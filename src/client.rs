use anyhow::Result;
use anyhow::anyhow;
use reqwest::StatusCode;
use serde_json::Map;
use serde_json::Value;

use crate::args::Cli;
use crate::auth::AuthStore;
use crate::security;

pub const USER_AGENT: &str = concat!("codex-imagegen-cli/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unauthorized: {body}")]
    Unauthorized { body: String },
    #[error("request failed with {status}: {body}")]
    Http { status: StatusCode, body: String },
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
}

pub struct ImageRequest {
    model: String,
    prompt: String,
    tool: Map<String, Value>,
}

impl ImageRequest {
    pub fn from_cli(cli: &Cli, prompt: String) -> Result<Self> {
        let mut tool = Map::new();
        tool.insert(
            "type".to_string(),
            Value::String("image_generation".to_string()),
        );
        tool.insert(
            "output_format".to_string(),
            Value::String(cli.format.as_str().to_string()),
        );

        insert_optional_string(&mut tool, "model", cli.image_model.as_deref());
        insert_optional_string(&mut tool, "size", cli.size.as_deref());
        insert_optional_string(&mut tool, "quality", cli.quality.as_deref());
        insert_optional_string(&mut tool, "background", cli.background.as_deref());
        insert_optional_string(&mut tool, "action", cli.action.as_deref());
        if let Some(compression) = cli.compression {
            if compression > 100 {
                return Err(anyhow!("compression must be between 0 and 100"));
            }
            tool.insert(
                "output_compression".to_string(),
                Value::Number(compression.into()),
            );
        }

        for raw in &cli.tool_params {
            let (key, value) = parse_tool_param(raw)?;
            tool.insert(key, value);
        }

        Ok(Self {
            model: cli.model.clone(),
            prompt,
            tool,
        })
    }

    pub fn output_format(&self) -> &str {
        self.tool
            .get("output_format")
            .and_then(Value::as_str)
            .unwrap_or("png")
    }

    pub fn to_body(&self) -> Value {
        serde_json::json!({
            "model": self.model,
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": self.prompt,
                        }
                    ]
                }
            ],
            "tools": [Value::Object(self.tool.clone())],
            "tool_choice": {"type": "image_generation"},
            "parallel_tool_calls": false,
            "store": false,
            "stream": false,
            "include": [],
        })
    }
}

pub async fn create_response(
    client: &reqwest::Client,
    base_url: &str,
    auth: &AuthStore,
    request: &ImageRequest,
) -> std::result::Result<Value, ApiError> {
    let url = format!("{}/responses", base_url.trim_end_matches('/'));
    let request_id = format!("codex-imagegen-{}", chrono::Utc::now().timestamp_millis());
    let builder = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-client-request-id", &request_id)
        .header("session_id", &request_id)
        .json(&request.to_body());
    let response = auth.add_headers(builder).send().await?;
    let status = response.status();
    let text = response.text().await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::Unauthorized {
            body: security::redact_known_secrets(text, &auth.secret_values()),
        });
    }
    if !status.is_success() {
        return Err(ApiError::Http {
            status,
            body: security::redact_known_secrets(text, &auth.secret_values()),
        });
    }
    serde_json::from_str(&text).map_err(|err| ApiError::Http {
        status,
        body: format!("failed to parse JSON response: {err}"),
    })
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value
        && !value.trim().is_empty()
    {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn parse_tool_param(raw: &str) -> Result<(String, Value)> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err(anyhow!("tool-param must use KEY=JSON_OR_TEXT format"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(anyhow!("tool-param key is empty"));
    }
    let value = value.trim();
    let parsed = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    Ok((key.to_string(), parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::OutputFormat;

    fn cli() -> Cli {
        Cli {
            prompt: Some("draw a test image".to_string()),
            prompt_file: None,
            prompt_arg: None,
            output: None,
            model: "gpt-5.5".to_string(),
            image_model: Some("gpt-image-2".to_string()),
            format: OutputFormat::Png,
            size: Some("1024x1024".to_string()),
            quality: Some("low".to_string()),
            compression: Some(80),
            background: Some("opaque".to_string()),
            action: Some("generate".to_string()),
            tool_params: vec!["moderation=\"low\"".to_string()],
            codex_home: None,
            base_url: None,
            timeout_secs: 600,
            json: false,
            dry_run: false,
        }
    }

    #[test]
    fn request_body_forces_image_generation_tool() {
        let request = ImageRequest::from_cli(&cli(), "draw a test image".to_string()).unwrap();
        let body = request.to_body();

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["tool_choice"]["type"], "image_generation");
        assert_eq!(body["tools"][0]["type"], "image_generation");
        assert_eq!(body["tools"][0]["model"], "gpt-image-2");
        assert_eq!(body["tools"][0]["output_format"], "png");
        assert_eq!(body["tools"][0]["output_compression"], 80);
        assert_eq!(body["tools"][0]["moderation"], "low");
    }
}
