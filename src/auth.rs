use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use chrono::SecondsFormat;
use chrono::Utc;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;

use crate::security;

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Clone)]
pub enum AuthMaterial {
    ChatGpt {
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
        is_fedramp_account: bool,
    },
    ApiKey {
        api_key: String,
    },
}

pub struct AuthStore {
    path: PathBuf,
    value: Value,
    material: AuthMaterial,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

impl AuthStore {
    pub fn load(codex_home: &Path) -> Result<Self> {
        let path = codex_home.join("auth.json");
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read Codex auth file: {}", path.display()))?;
        let value: Value = serde_json::from_str(&raw).context("failed to parse Codex auth JSON")?;
        let material = parse_auth_material(&value)?;
        Ok(Self {
            path,
            value,
            material,
        })
    }

    pub fn default_base_url(&self) -> &'static str {
        match self.material {
            AuthMaterial::ChatGpt { .. } => DEFAULT_CODEX_BASE_URL,
            AuthMaterial::ApiKey { .. } => DEFAULT_OPENAI_BASE_URL,
        }
    }

    pub fn can_refresh(&self) -> bool {
        matches!(self.material, AuthMaterial::ChatGpt { .. })
    }

    pub fn add_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.material {
            AuthMaterial::ChatGpt {
                access_token,
                account_id,
                is_fedramp_account,
                ..
            } => {
                let request = request.bearer_auth(access_token);
                let request = if let Some(account_id) = account_id {
                    request.header("ChatGPT-Account-ID", account_id)
                } else {
                    request
                };
                if *is_fedramp_account {
                    request.header("X-OpenAI-Fedramp", "true")
                } else {
                    request
                }
            }
            AuthMaterial::ApiKey { api_key } => request.bearer_auth(api_key),
        }
    }

    pub fn secret_values(&self) -> Vec<String> {
        match &self.material {
            AuthMaterial::ChatGpt {
                access_token,
                refresh_token,
                ..
            } => vec![access_token.clone(), refresh_token.clone()],
            AuthMaterial::ApiKey { api_key } => vec![api_key.clone()],
        }
    }

    pub async fn refresh_chatgpt_token(&mut self, client: &reqwest::Client) -> Result<()> {
        let refresh_token = match &self.material {
            AuthMaterial::ChatGpt { refresh_token, .. } => refresh_token.clone(),
            AuthMaterial::ApiKey { .. } => return Ok(()),
        };

        let response = client
            .post(REFRESH_TOKEN_URL)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "client_id": CLIENT_ID,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .send()
            .await
            .context("failed to request ChatGPT token refresh")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read token refresh response")?;

        if status != StatusCode::OK {
            let safe = security::redact_known_secrets(body, &self.secret_values());
            return Err(anyhow!("refresh endpoint returned {status}: {safe}"));
        }

        let refresh_response: RefreshResponse =
            serde_json::from_str(&body).context("failed to parse token refresh response")?;
        self.persist_refresh_response(refresh_response)?;
        self.material = parse_auth_material(&self.value)?;
        Ok(())
    }

    fn persist_refresh_response(&mut self, refresh: RefreshResponse) -> Result<()> {
        let tokens = self
            .value
            .get_mut("tokens")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow!("Codex auth JSON has no tokens object"))?;

        if let Some(id_token) = refresh.id_token {
            tokens.insert("id_token".to_string(), Value::String(id_token));
        }
        if let Some(access_token) = refresh.access_token {
            tokens.insert("access_token".to_string(), Value::String(access_token));
        }
        if let Some(refresh_token) = refresh.refresh_token {
            tokens.insert("refresh_token".to_string(), Value::String(refresh_token));
        }

        self.value["last_refresh"] =
            Value::String(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
        write_auth_json(&self.path, &self.value)
    }
}

pub fn resolve_codex_home(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home).join(".codex"))
}

fn parse_auth_material(value: &Value) -> Result<AuthMaterial> {
    if let Some(tokens) = value.get("tokens").and_then(Value::as_object) {
        let access_token = required_string(tokens.get("access_token"), "tokens.access_token")?;
        let refresh_token = required_string(tokens.get("refresh_token"), "tokens.refresh_token")?;
        let account_id = tokens
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_fedramp_account = tokens
            .get("id_token")
            .and_then(Value::as_object)
            .and_then(|id_token| id_token.get("chatgpt_account_is_fedramp"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(AuthMaterial::ChatGpt {
            access_token,
            refresh_token,
            account_id,
            is_fedramp_account,
        });
    }

    if let Some(api_key) = value.get("OPENAI_API_KEY").and_then(Value::as_str)
        && !api_key.trim().is_empty()
    {
        return Ok(AuthMaterial::ApiKey {
            api_key: api_key.to_string(),
        });
    }

    Err(anyhow!(
        "Codex auth JSON contains neither ChatGPT tokens nor OPENAI_API_KEY"
    ))
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing {field}"))?;
    if value.trim().is_empty() {
        return Err(anyhow!("{field} is empty"));
    }
    Ok(value.to_string())
}

fn write_auth_json(path: &Path, value: &Value) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("auth path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("auth path has no valid file name: {}", path.display()))?;
    let temp_path = parent.join(format!(".{file_name}.codex-imagegen.tmp"));
    let bytes = serde_json::to_vec_pretty(value)?;

    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_chatgpt_auth() {
        let value = serde_json::json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "access_token": "access-token-123456",
                "refresh_token": "refresh-token-123456",
                "account_id": "account-1"
            },
            "last_refresh": "2026-05-01T00:00:00Z"
        });

        let material = parse_auth_material(&value).expect("parse auth");
        match material {
            AuthMaterial::ChatGpt {
                access_token,
                refresh_token,
                account_id,
                ..
            } => {
                assert_eq!(access_token, "access-token-123456");
                assert_eq!(refresh_token, "refresh-token-123456");
                assert_eq!(account_id.as_deref(), Some("account-1"));
            }
            AuthMaterial::ApiKey { .. } => panic!("expected ChatGPT auth"),
        }
    }

    #[test]
    fn falls_back_to_api_key_auth() {
        let value = serde_json::json!({
            "auth_mode": "api_key",
            "OPENAI_API_KEY": "dummy-api-key-123456"
        });

        let material = parse_auth_material(&value).expect("parse auth");
        match material {
            AuthMaterial::ApiKey { api_key } => assert_eq!(api_key, "dummy-api-key-123456"),
            AuthMaterial::ChatGpt { .. } => panic!("expected API key auth"),
        }
    }
}
