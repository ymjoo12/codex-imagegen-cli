use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use base64::Engine as _;
use chrono::SecondsFormat;
use chrono::Utc;
use keyring::Entry;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

use crate::args::AuthSource;
use crate::args::AuthStoreMode;
use crate::security;

const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_MODEL: &str = "gpt-5.5";
const TOKEN_REFRESH_INTERVAL_DAYS: i64 = 8;
const KEYRING_SERVICE: &str = "Codex Auth";
const CONFIG_FILE: &str = "config.toml";
const AUTH_FILE: &str = "auth.json";
const INSTALLATION_ID_FILE: &str = "installation_id";
const CODEX_API_KEY_ENV_VAR: &str = "CODEX_API_KEY";
const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
const CODEX_AGENT_IDENTITY_ENV_VAR: &str = "CODEX_AGENT_IDENTITY";

#[derive(Clone)]
pub enum AuthMaterial {
    ChatGpt {
        access_token: String,
        refresh_token: Option<String>,
        account_id: Option<String>,
        is_fedramp_account: bool,
    },
    ApiKey {
        api_key: String,
    },
    ProviderBearer {
        token: String,
    },
}

pub struct AuthStore {
    codex_home: PathBuf,
    auth_store_mode: Option<PersistentAuthStoreMode>,
    save_target: AuthSaveTarget,
    default_base_url: String,
    provider_headers: Vec<(String, String)>,
    query_params: Vec<(String, String)>,
    value: Value,
    material: AuthMaterial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PersistentAuthStoreMode {
    Auto,
    File,
    Keyring,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthSaveTarget {
    None,
    Auto,
    File,
    Keyring,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

struct ProviderRequestConfig {
    base_url: String,
    token: Option<String>,
    headers: Vec<(String, String)>,
    query_params: Vec<(String, String)>,
}

#[derive(Debug, Deserialize, Default)]
struct CodexConfig {
    profile: Option<String>,
    model: Option<String>,
    model_provider: Option<String>,
    chatgpt_base_url: Option<String>,
    cli_auth_credentials_store: Option<String>,
    #[serde(default)]
    profiles: HashMap<String, CodexProfile>,
    #[serde(default)]
    model_providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct CodexProfile {
    model: Option<String>,
    model_provider: Option<String>,
    chatgpt_base_url: Option<String>,
}

#[derive(Debug, Default)]
struct EffectiveCodexConfig {
    model: Option<String>,
    model_provider: Option<String>,
    chatgpt_base_url: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
enum ReloadOutcome {
    Changed,
    Unchanged,
    Skipped,
}

#[derive(Debug, Deserialize, Default)]
struct ProviderConfig {
    base_url: Option<String>,
    env_key: Option<String>,
    experimental_bearer_token: Option<String>,
    auth: Option<ProviderCommandAuth>,
    #[serde(default)]
    http_headers: HashMap<String, String>,
    #[serde(default)]
    env_http_headers: HashMap<String, String>,
    #[serde(default)]
    query_params: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ProviderCommandAuth {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    timeout_ms: Option<u64>,
    cwd: Option<PathBuf>,
}

impl AuthStore {
    pub fn load(
        codex_home: &Path,
        requested_mode: AuthStoreMode,
        auth_source: AuthSource,
        profile: Option<&str>,
    ) -> Result<Self> {
        let effective_config = read_effective_codex_config(codex_home, profile)?;
        let provider_config = if matches!(auth_source, AuthSource::Codex | AuthSource::Provider) {
            read_provider_request_config(codex_home, &effective_config)?
        } else {
            None
        };

        if let Some(provider) = provider_config.as_ref() {
            if provider.token.is_some() {
                return Self::load_provider(codex_home, provider);
            }
            if matches!(auth_source, AuthSource::Provider) {
                return Err(anyhow!(
                    "Codex model provider is configured without bearer auth"
                ));
            }
        }

        if let Some(api_key) = read_api_key_from_env() {
            let value = serde_json::json!({
                "auth_mode": "api_key",
                "OPENAI_API_KEY": api_key,
            });
            let material = parse_auth_material(&value)?;
            let mut store = Self {
                codex_home: codex_home.to_path_buf(),
                auth_store_mode: None,
                save_target: AuthSaveTarget::None,
                default_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
                provider_headers: Vec::new(),
                query_params: Vec::new(),
                value,
                material,
            };
            store.apply_provider_config(provider_config);
            return Ok(store);
        }

        if std::env::var_os(CODEX_AGENT_IDENTITY_ENV_VAR).is_some() {
            return Err(anyhow!(
                "CODEX_AGENT_IDENTITY is set, but this standalone CLI cannot yet perform Codex agent-identity request signing"
            ));
        }

        let mode = resolve_persistent_auth_store_mode(codex_home, requested_mode)?;
        let value = load_auth_value(codex_home, mode)?;
        let material = parse_auth_material(&value)?;
        let mut store = Self {
            codex_home: codex_home.to_path_buf(),
            auth_store_mode: Some(mode),
            save_target: match mode {
                PersistentAuthStoreMode::Auto => AuthSaveTarget::Auto,
                PersistentAuthStoreMode::File => AuthSaveTarget::File,
                PersistentAuthStoreMode::Keyring => AuthSaveTarget::Keyring,
            },
            default_base_url: default_base_url_for_material(
                &material,
                effective_config.chatgpt_base_url.as_deref(),
            )
            .to_string(),
            provider_headers: Vec::new(),
            query_params: Vec::new(),
            value,
            material,
        };
        store.apply_provider_config(provider_config);
        Ok(store)
    }

    fn load_provider(codex_home: &Path, provider: &ProviderRequestConfig) -> Result<Self> {
        let base_url = provider.base_url.clone();
        let value = serde_json::json!({
            "auth_mode": "provider_bearer",
            "base_url": base_url,
        });
        let token = provider
            .token
            .clone()
            .ok_or_else(|| anyhow!("provider bearer token is missing"))?;
        Ok(Self {
            codex_home: codex_home.to_path_buf(),
            auth_store_mode: None,
            save_target: AuthSaveTarget::None,
            default_base_url: base_url,
            provider_headers: provider.headers.clone(),
            query_params: provider.query_params.clone(),
            value,
            material: AuthMaterial::ProviderBearer { token },
        })
    }

    fn apply_provider_config(&mut self, provider: Option<ProviderRequestConfig>) {
        let Some(provider) = provider else {
            return;
        };
        self.default_base_url = provider.base_url;
        self.provider_headers = provider.headers;
        self.query_params = provider.query_params;
    }

    pub fn default_base_url(&self) -> &str {
        &self.default_base_url
    }

    pub fn query_params(&self) -> &[(String, String)] {
        &self.query_params
    }

    pub fn can_refresh(&self) -> bool {
        matches!(
            &self.material,
            AuthMaterial::ChatGpt {
                refresh_token: Some(refresh_token),
                ..
            } if !refresh_token.trim().is_empty()
        )
    }

    pub fn is_stale_chatgpt_auth(&self) -> bool {
        let AuthMaterial::ChatGpt { access_token, .. } = &self.material else {
            return false;
        };

        if access_token_is_expired(access_token).unwrap_or(false) {
            return true;
        }

        let Some(last_refresh) = self.value.get("last_refresh").and_then(Value::as_str) else {
            return false;
        };
        let Ok(last_refresh) = chrono::DateTime::parse_from_rfc3339(last_refresh) else {
            return false;
        };
        last_refresh.with_timezone(&Utc)
            < Utc::now() - chrono::Duration::days(TOKEN_REFRESH_INTERVAL_DAYS)
    }

    pub fn add_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let request = match &self.material {
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
            AuthMaterial::ProviderBearer { token } => request.bearer_auth(token),
        };
        self.provider_headers
            .iter()
            .fold(request, |request, (name, value)| {
                request.header(name.as_str(), value.as_str())
            })
    }

    fn provider_secret_values(&self) -> Vec<String> {
        self.provider_headers
            .iter()
            .chain(self.query_params.iter())
            .map(|(_, value)| value.clone())
            .filter(|value| !value.trim().is_empty())
            .collect()
    }

    pub fn secret_values(&self) -> Vec<String> {
        let mut values = match &self.material {
            AuthMaterial::ChatGpt {
                access_token,
                refresh_token,
                ..
            } => {
                let mut values = vec![access_token.clone()];
                if let Some(refresh_token) = refresh_token {
                    values.push(refresh_token.clone());
                }
                values
            }
            AuthMaterial::ApiKey { api_key } => vec![api_key.clone()],
            AuthMaterial::ProviderBearer { token } => vec![token.clone()],
        };
        values.extend(self.provider_secret_values());
        values
    }

    pub async fn refresh_chatgpt_token(&mut self, client: &reqwest::Client) -> Result<()> {
        match self.reload_changed_auth()? {
            ReloadOutcome::Changed => return Ok(()),
            ReloadOutcome::Unchanged | ReloadOutcome::Skipped => {}
        }
        self.refresh_chatgpt_token_from_authority(client).await
    }

    fn reload_changed_auth(&mut self) -> Result<ReloadOutcome> {
        let Some(mode) = self.auth_store_mode else {
            return Ok(ReloadOutcome::Skipped);
        };
        let expected_account_id = match self.account_id() {
            Some(account_id) => account_id,
            None => return Ok(ReloadOutcome::Skipped),
        };
        let value = load_auth_value(&self.codex_home, mode)?;
        let material = parse_auth_material(&value)?;
        let loaded_account_id = account_id_for_material(&material);
        if loaded_account_id.as_deref() != Some(expected_account_id.as_str()) {
            let found = loaded_account_id.unwrap_or_else(|| "unknown".to_string());
            return Err(anyhow!(
                "stored Codex auth account changed while refreshing tokens; expected account {expected_account_id}, found account {found}"
            ));
        }

        if value == self.value {
            return Ok(ReloadOutcome::Unchanged);
        }

        self.value = value;
        self.material = material;
        Ok(ReloadOutcome::Changed)
    }

    fn account_id(&self) -> Option<String> {
        account_id_for_material(&self.material)
    }

    async fn refresh_chatgpt_token_from_authority(
        &mut self,
        client: &reqwest::Client,
    ) -> Result<()> {
        let refresh_token = match &self.material {
            AuthMaterial::ChatGpt {
                refresh_token: Some(refresh_token),
                ..
            } if !refresh_token.trim().is_empty() => refresh_token.clone(),
            AuthMaterial::ApiKey { .. } | AuthMaterial::ProviderBearer { .. } => return Ok(()),
            AuthMaterial::ChatGpt { .. } => {
                return Err(anyhow!("ChatGPT auth has no managed refresh token"));
            }
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
        self.save_auth_value()
    }

    fn save_auth_value(&self) -> Result<()> {
        match self.save_target {
            AuthSaveTarget::None => Ok(()),
            AuthSaveTarget::File => write_auth_json(&self.codex_home.join(AUTH_FILE), &self.value),
            AuthSaveTarget::Keyring => write_keyring_auth_json(&self.codex_home, &self.value),
            AuthSaveTarget::Auto => match write_keyring_auth_json(&self.codex_home, &self.value) {
                Ok(()) => Ok(()),
                Err(_) => write_auth_json(&self.codex_home.join(AUTH_FILE), &self.value),
            },
        }
    }
}

fn default_base_url_for_material<'a>(
    material: &AuthMaterial,
    chatgpt_base_url: Option<&'a str>,
) -> &'a str {
    match material {
        AuthMaterial::ChatGpt { .. } => chatgpt_base_url.unwrap_or(DEFAULT_CODEX_BASE_URL),
        AuthMaterial::ApiKey { .. } | AuthMaterial::ProviderBearer { .. } => {
            DEFAULT_OPENAI_BASE_URL
        }
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

pub fn resolve_installation_id(codex_home: &Path) -> Result<String> {
    fs::create_dir_all(codex_home)
        .with_context(|| format!("failed to create Codex home: {}", codex_home.display()))?;
    let path = codex_home.join(INSTALLATION_ID_FILE);
    let existing = fs::read_to_string(&path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| uuid::Uuid::parse_str(value).is_ok());
    if let Some(existing) = existing {
        return Ok(uuid::Uuid::parse_str(&existing)?.to_string());
    }

    let installation_id = uuid::Uuid::new_v4().to_string();
    fs::write(&path, &installation_id)
        .with_context(|| format!("failed to write installation id: {}", path.display()))?;
    Ok(installation_id)
}

pub fn resolve_effective_model(
    codex_home: &Path,
    profile: Option<&str>,
    cli_model: Option<&str>,
) -> Result<String> {
    if let Some(model) = cli_model.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(model.to_string());
    }

    let effective = read_effective_codex_config(codex_home, profile)?;
    Ok(effective
        .model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string()))
}

fn parse_auth_material(value: &Value) -> Result<AuthMaterial> {
    if matches!(auth_mode(value), Some("api_key" | "apikey")) {
        return parse_api_key_material(value);
    }

    if matches!(auth_mode(value), Some("agentIdentity" | "agent_identity")) {
        return Err(anyhow!(
            "agent_identity auth requires Codex request signing that is not implemented in this standalone CLI"
        ));
    }

    if let Some(tokens) = value.get("tokens").and_then(Value::as_object) {
        let access_token = required_string(tokens.get("access_token"), "tokens.access_token")?;
        let refresh_token = tokens
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let account_id = tokens
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let is_fedramp_account = tokens.get("id_token").is_some_and(is_fedramp_id_token);
        return Ok(AuthMaterial::ChatGpt {
            access_token,
            refresh_token,
            account_id,
            is_fedramp_account,
        });
    }

    parse_api_key_material(value)
}

fn parse_api_key_material(value: &Value) -> Result<AuthMaterial> {
    if let Some(api_key) = value.get(OPENAI_API_KEY_ENV_VAR).and_then(Value::as_str)
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

fn auth_mode(value: &Value) -> Option<&str> {
    value.get("auth_mode").and_then(Value::as_str)
}

fn account_id_for_material(material: &AuthMaterial) -> Option<String> {
    match material {
        AuthMaterial::ChatGpt { account_id, .. } => account_id.clone(),
        AuthMaterial::ApiKey { .. } | AuthMaterial::ProviderBearer { .. } => None,
    }
}

fn access_token_is_expired(token: &str) -> Option<bool> {
    let mut parts = token.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims = serde_json::from_slice::<Value>(&decoded).ok()?;
    let expires_at = claims.get("exp")?.as_i64()?;
    let expires_at = chrono::DateTime::<Utc>::from_timestamp(expires_at, 0)?;
    Some(expires_at <= Utc::now())
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

fn read_api_key_from_env() -> Option<String> {
    [CODEX_API_KEY_ENV_VAR, OPENAI_API_KEY_ENV_VAR]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn read_provider_request_config(
    codex_home: &Path,
    effective: &EffectiveCodexConfig,
) -> Result<Option<ProviderRequestConfig>> {
    let Some(config) = read_codex_config(codex_home)? else {
        return Ok(None);
    };
    let provider_name = effective
        .model_provider
        .clone()
        .unwrap_or_else(|| "openai".to_string());
    let Some(provider) = config.model_providers.get(&provider_name) else {
        return Ok(None);
    };
    let Some(base_url) = provider.base_url.clone() else {
        return Ok(None);
    };
    let token = provider
        .experimental_bearer_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            provider.env_key.as_deref().and_then(|env_key| {
                std::env::var(env_key)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        });
    let token = match (token, provider.auth.as_ref()) {
        (Some(token), _) => Some(token),
        (None, Some(auth)) => Some(run_provider_auth_command(auth)?),
        (None, None) => None,
    };

    Ok(Some(ProviderRequestConfig {
        base_url,
        token,
        headers: provider_headers(provider),
        query_params: provider
            .query_params
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
    }))
}

fn read_effective_codex_config(
    codex_home: &Path,
    requested_profile: Option<&str>,
) -> Result<EffectiveCodexConfig> {
    let Some(config) = read_codex_config(codex_home)? else {
        return Ok(EffectiveCodexConfig::default());
    };
    let active_profile_name = requested_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(config.profile.clone());
    let active_profile = match active_profile_name.as_deref() {
        Some(name) => Some(config.profiles.get(name).ok_or_else(|| {
            anyhow!(
                "config profile `{name}` was not found in {}",
                codex_home.join(CONFIG_FILE).display()
            )
        })?),
        None => None,
    };

    Ok(EffectiveCodexConfig {
        model: active_profile
            .and_then(|profile| profile.model.clone())
            .or(config.model),
        model_provider: active_profile
            .and_then(|profile| profile.model_provider.clone())
            .or(config.model_provider),
        chatgpt_base_url: active_profile
            .and_then(|profile| profile.chatgpt_base_url.clone())
            .or(config.chatgpt_base_url),
    })
}

fn resolve_persistent_auth_store_mode(
    codex_home: &Path,
    requested_mode: AuthStoreMode,
) -> Result<PersistentAuthStoreMode> {
    match requested_mode {
        AuthStoreMode::Auto => Ok(PersistentAuthStoreMode::Auto),
        AuthStoreMode::File => Ok(PersistentAuthStoreMode::File),
        AuthStoreMode::Keyring => Ok(PersistentAuthStoreMode::Keyring),
        AuthStoreMode::Codex => read_configured_auth_store_mode(codex_home),
    }
}

fn read_configured_auth_store_mode(codex_home: &Path) -> Result<PersistentAuthStoreMode> {
    let Some(config) = read_codex_config(codex_home)? else {
        return Ok(PersistentAuthStoreMode::File);
    };
    let Some(value) = config.cli_auth_credentials_store else {
        return Ok(PersistentAuthStoreMode::File);
    };

    match value.as_str() {
        "auto" => Ok(PersistentAuthStoreMode::Auto),
        "file" => Ok(PersistentAuthStoreMode::File),
        "keyring" => Ok(PersistentAuthStoreMode::Keyring),
        "ephemeral" => Err(anyhow!(
            "cli_auth_credentials_store = \"ephemeral\" is in-memory only and cannot be reused by a standalone CLI process"
        )),
        other => Err(anyhow!(
            "unsupported cli_auth_credentials_store value in {}: {other}",
            codex_home.join(CONFIG_FILE).display()
        )),
    }
}

fn read_codex_config(codex_home: &Path) -> Result<Option<CodexConfig>> {
    let path = codex_home.join(CONFIG_FILE);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read Codex config: {}", path.display()));
        }
    };
    toml::from_str(&raw)
        .map(Some)
        .with_context(|| format!("failed to parse Codex config TOML: {}", path.display()))
}

fn provider_headers(provider: &ProviderConfig) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = provider
        .http_headers
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    headers.extend(
        provider
            .env_http_headers
            .iter()
            .filter_map(|(name, env_var)| {
                std::env::var(env_var)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .map(|value| (name.clone(), value))
            }),
    );
    headers
}

fn run_provider_auth_command(config: &ProviderCommandAuth) -> Result<String> {
    if config.command.trim().is_empty() {
        return Err(anyhow!("provider auth.command is empty"));
    }
    let cwd = match &config.cwd {
        Some(cwd) => cwd.clone(),
        None => std::env::current_dir().context("failed to resolve current directory")?,
    };
    let program = resolve_provider_auth_program(&config.command, &cwd);
    let mut child = Command::new(&program)
        .args(&config.args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("provider auth command `{}` failed to start", config.command))?;
    let timeout = Duration::from_millis(config.timeout_ms.unwrap_or(5_000).max(1));
    let started_at = Instant::now();

    loop {
        if child.try_wait()?.is_some() {
            let output = child.wait_with_output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let suffix = if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                };
                return Err(anyhow!(
                    "provider auth command `{}` exited with status {}{}",
                    config.command,
                    output.status,
                    suffix
                ));
            }
            let stdout = String::from_utf8(output.stdout).with_context(|| {
                format!(
                    "provider auth command `{}` wrote non-UTF-8 stdout",
                    config.command
                )
            })?;
            let token = stdout.trim().to_string();
            if token.is_empty() {
                return Err(anyhow!(
                    "provider auth command `{}` produced an empty token",
                    config.command
                ));
            }
            return Ok(token);
        }
        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "provider auth command `{}` timed out after {} ms",
                config.command,
                timeout.as_millis()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn resolve_provider_auth_program(command: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(command);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    if path.components().count() > 1 {
        return cwd.join(path);
    }
    PathBuf::from(command)
}

fn load_auth_value(codex_home: &Path, mode: PersistentAuthStoreMode) -> Result<Value> {
    match mode {
        PersistentAuthStoreMode::File => read_auth_json(&codex_home.join(AUTH_FILE)),
        PersistentAuthStoreMode::Keyring => read_keyring_auth_json(codex_home)?.ok_or_else(|| {
            anyhow!(
                "Codex keyring entry was not found for {}",
                codex_home.display()
            )
        }),
        PersistentAuthStoreMode::Auto => match read_keyring_auth_json(codex_home) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => read_auth_json(&codex_home.join(AUTH_FILE)),
            Err(_) => read_auth_json(&codex_home.join(AUTH_FILE)),
        },
    }
}

fn read_auth_json(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read Codex auth file: {}", path.display()))?;
    serde_json::from_str(&raw).context("failed to parse Codex auth JSON")
}

fn read_keyring_auth_json(codex_home: &Path) -> Result<Option<Value>> {
    let account = codex_keyring_account(codex_home);
    let entry = Entry::new(KEYRING_SERVICE, &account)
        .with_context(|| format!("failed to open Codex keyring entry {account}"))?;
    let raw = match entry.get_password() {
        Ok(raw) => raw,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(err) => {
            return Err(anyhow!(
                "failed to read Codex keyring entry {account}: {err}"
            ));
        }
    };
    serde_json::from_str(&raw)
        .map(Some)
        .context("failed to parse Codex keyring auth JSON")
}

fn write_keyring_auth_json(codex_home: &Path, value: &Value) -> Result<()> {
    let account = codex_keyring_account(codex_home);
    let entry = Entry::new(KEYRING_SERVICE, &account)
        .with_context(|| format!("failed to open Codex keyring entry {account}"))?;
    let raw = serde_json::to_string(value)?;
    entry
        .set_password(&raw)
        .with_context(|| format!("failed to write Codex keyring entry {account}"))?;
    delete_auth_json_if_exists(codex_home)?;
    Ok(())
}

fn delete_auth_json_if_exists(codex_home: &Path) -> Result<()> {
    match fs::remove_file(codex_home.join(AUTH_FILE)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).context("failed to remove Codex auth.json fallback"),
    }
}

pub fn codex_keyring_account(codex_home: &Path) -> String {
    let canonical = codex_home
        .canonicalize()
        .unwrap_or_else(|_| codex_home.to_path_buf());
    codex_keyring_account_for_path(canonical.to_string_lossy().as_ref())
}

fn codex_keyring_account_for_path(path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let truncated = hex.get(..16).unwrap_or(&hex);
    format!("cli|{truncated}")
}

fn is_fedramp_id_token(value: &Value) -> bool {
    if let Some(object) = value.as_object() {
        return object
            .get("chatgpt_account_is_fedramp")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    }

    let Some(jwt) = value.as_str() else {
        return false;
    };
    let mut parts = jwt.split('.');
    let (Some(_header), Some(payload), Some(_signature)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(claims) = serde_json::from_slice::<Value>(&decoded) else {
        return false;
    };
    claims
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_is_fedramp"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
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
                assert_eq!(refresh_token.as_deref(), Some("refresh-token-123456"));
                assert_eq!(account_id.as_deref(), Some("account-1"));
            }
            AuthMaterial::ApiKey { .. } => panic!("expected ChatGPT auth"),
            AuthMaterial::ProviderBearer { .. } => panic!("expected ChatGPT auth"),
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
            AuthMaterial::ProviderBearer { .. } => panic!("expected API key auth"),
        }
    }

    #[test]
    fn computes_codex_keyring_account() {
        assert_eq!(
            codex_keyring_account_for_path("/tmp/codex-home"),
            "cli|c790889e29f35b54"
        );
    }

    #[test]
    fn reads_configured_store_mode() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"
            model = "gpt-5.5"
            cli_auth_credentials_store = "auto"
            [profiles.other]
            cli_auth_credentials_store = "file"
        "#,
        )
        .expect("write config");

        assert_eq!(
            read_configured_auth_store_mode(dir.path()).expect("read store mode"),
            PersistentAuthStoreMode::Auto
        );
    }

    #[test]
    fn reads_provider_auth_config_fields() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"
            model_provider = "custom_gateway"

            [model_providers.custom_gateway]
            base_url = "https://api.example.invalid/v1"
            experimental_bearer_token = "dummy-provider-token-123456"
            query_params = { "api-version" = "2026-01-01" }
            http_headers = { "x-test" = "static" }
        "#,
        )
        .expect("write config");

        let effective =
            read_effective_codex_config(dir.path(), None).expect("read effective config");
        let provider = read_provider_request_config(dir.path(), &effective)
            .expect("read provider config")
            .expect("provider config");
        assert_eq!(provider.base_url.as_str(), "https://api.example.invalid/v1");
        assert_eq!(
            provider.token.as_deref(),
            Some("dummy-provider-token-123456")
        );
        assert!(
            provider
                .headers
                .contains(&("x-test".to_string(), "static".to_string()))
        );
        assert!(
            provider
                .query_params
                .contains(&("api-version".to_string(), "2026-01-01".to_string()))
        );
    }

    #[test]
    fn config_profile_overrides_model_and_provider() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"
            model_provider = "custom_gateway"
            model = "gpt-5.5"

            [model_providers.custom_gateway]
            base_url = "https://api.example.invalid/v1"
            experimental_bearer_token = "dummy-provider-token-123456"

            [profiles.openai]
            model_provider = "openai"
            model = "gpt-5.4"
        "#,
        )
        .expect("write config");

        let model = resolve_effective_model(dir.path(), Some("openai"), None)
            .expect("resolve effective model");
        let effective =
            read_effective_codex_config(dir.path(), Some("openai")).expect("read effective config");
        let provider =
            read_provider_request_config(dir.path(), &effective).expect("read provider config");

        assert_eq!(model, "gpt-5.4");
        assert!(provider.is_none());
    }

    #[test]
    fn config_profile_selects_custom_provider() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"
            model_provider = "openai"
            model = "gpt-5.5"

            [model_providers.custom_gateway]
            base_url = "https://api.example.invalid/v1"
            experimental_bearer_token = "dummy-provider-token-123456"

            [profiles.custom_gateway]
            model_provider = "custom_gateway"
            model = "gpt-5.5"
        "#,
        )
        .expect("write config");

        let effective = read_effective_codex_config(dir.path(), Some("custom_gateway"))
            .expect("read effective config");
        let provider = read_provider_request_config(dir.path(), &effective)
            .expect("read provider config")
            .expect("provider config");

        assert_eq!(provider.base_url, "https://api.example.invalid/v1");
        assert_eq!(
            provider.token.as_deref(),
            Some("dummy-provider-token-123456")
        );
    }

    #[test]
    fn applies_provider_routing_without_provider_bearer_to_managed_auth() {
        let dir = tempfile::tempdir().expect("create temp dir");
        fs::write(
            dir.path().join(CONFIG_FILE),
            r#"
            model_provider = "corp"

            [model_providers.corp]
            base_url = "https://api.example.invalid/v1"
            query_params = { "api-version" = "2026-01-01" }
        "#,
        )
        .expect("write config");
        fs::write(
            dir.path().join(AUTH_FILE),
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"dummy-api-key-123456"}"#,
        )
        .expect("write auth");

        let store = AuthStore::load(dir.path(), AuthStoreMode::File, AuthSource::Codex, None)
            .expect("load auth store");

        assert_eq!(store.default_base_url(), "https://api.example.invalid/v1");
        assert!(
            store
                .query_params()
                .contains(&("api-version".to_string(), "2026-01-01".to_string()))
        );
    }
}
