use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use base64::Engine;
use reqwest::StatusCode;
use serde_json::Map;
use serde_json::Value;

use crate::args::Cli;
use crate::args::ToolChoice;
use crate::auth::AuthStore;
use crate::response;
use crate::security;

pub const USER_AGENT: &str = concat!("codex-imagegen-cli/", env!("CARGO_PKG_VERSION"));
const DEFAULT_INSTRUCTIONS: &str =
    "You are Codex. Generate or edit images by using the provided image_generation tool.";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unauthorized: {body}")]
    Unauthorized { body: String },
    #[error("request failed with {status}: {body}")]
    Http { status: StatusCode, body: String },
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
}

#[derive(Debug)]
pub struct ImageRequest {
    model: String,
    instructions: String,
    prompt: String,
    input_images: Vec<InputImage>,
    tool: Map<String, Value>,
    tool_choice: ToolChoice,
    stream: bool,
    installation_id: Option<String>,
}

#[derive(Clone, Debug)]
struct InputImage {
    media_type: &'static str,
    data_base64: String,
}

impl InputImage {
    fn from_path(path: &Path) -> Result<Self> {
        let media_type = media_type_for_path(path)?;
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read input image: {}", path.display()))?;
        if bytes.is_empty() {
            bail!("input image is empty: {}", path.display());
        }
        Ok(Self {
            media_type,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }

    fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.media_type, self.data_base64)
    }

    fn redacted_data_url(&self) -> String {
        format!(
            "data:{};base64,<redacted:{} chars>",
            self.media_type,
            self.data_base64.len()
        )
    }
}

impl ImageRequest {
    pub fn from_cli(cli: &Cli, prompt: String, model: String) -> Result<Self> {
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

        let instructions = cli
            .instructions
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_INSTRUCTIONS)
            .to_string();
        let input_images = cli
            .input_images
            .iter()
            .map(|path| InputImage::from_path(path))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            model,
            instructions,
            prompt,
            input_images,
            tool,
            tool_choice: cli.tool_choice,
            stream: !cli.no_stream,
            installation_id: None,
        })
    }

    pub fn set_codex_identity(&mut self, installation_id: String) {
        self.installation_id = Some(installation_id);
    }

    pub fn output_format(&self) -> &str {
        self.tool
            .get("output_format")
            .and_then(Value::as_str)
            .unwrap_or("png")
    }

    pub fn stream(&self) -> bool {
        self.stream
    }

    #[cfg(test)]
    pub fn to_body(&self) -> Value {
        self.to_body_with_prompt_cache_key(None)
    }

    pub fn to_redacted_body(&self) -> Value {
        self.build_body(None, true)
    }

    fn to_body_with_prompt_cache_key(&self, prompt_cache_key: Option<&str>) -> Value {
        self.build_body(prompt_cache_key, false)
    }

    fn build_body(&self, prompt_cache_key: Option<&str>, redact_images: bool) -> Value {
        let tool_choice = match self.tool_choice {
            ToolChoice::Auto => Value::String("auto".to_string()),
            ToolChoice::ImageGeneration => serde_json::json!({"type": "image_generation"}),
        };
        let mut content = vec![serde_json::json!({
            "type": "input_text",
            "text": self.prompt,
        })];
        content.extend(self.input_images.iter().map(|image| {
            serde_json::json!({
                "type": "input_image",
                "image_url": if redact_images {
                    image.redacted_data_url()
                } else {
                    image.data_url()
                },
            })
        }));
        let mut body = serde_json::json!({
            "model": self.model,
            "instructions": self.instructions,
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": content
                }
            ],
            "tools": [Value::Object(self.tool.clone())],
            "tool_choice": tool_choice,
            "parallel_tool_calls": false,
            "store": false,
            "stream": self.stream,
            "include": [],
        });

        if let Some(installation_id) = self.installation_id.as_deref() {
            body["client_metadata"] = serde_json::json!({
                "x-codex-installation-id": installation_id,
            });
        }

        if let Some(prompt_cache_key) = prompt_cache_key {
            body["prompt_cache_key"] = Value::String(prompt_cache_key.to_string());
        }

        body
    }
}

fn media_type_for_path(path: &Path) -> Result<&'static str> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("png") => Ok("image/png"),
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("webp") => Ok("image/webp"),
        _ => bail!(
            "unsupported input image format for {}: use .png, .jpg, .jpeg, or .webp",
            path.display()
        ),
    }
}

pub async fn create_response(
    client: &reqwest::Client,
    base_url: &str,
    auth: &AuthStore,
    request: &ImageRequest,
) -> std::result::Result<Value, ApiError> {
    let url = format!("{}/responses", base_url.trim_end_matches('/'));
    let conversation_id = uuid::Uuid::now_v7().to_string();
    let builder = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("x-client-request-id", &conversation_id)
        .header("session_id", &conversation_id)
        .json(&request.to_body_with_prompt_cache_key(Some(&conversation_id)));
    let builder = if auth.query_params().is_empty() {
        builder
    } else {
        builder.query(auth.query_params())
    };
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
    if request.stream() {
        response::parse_sse_response(&text).map_err(|err| ApiError::Http {
            status,
            body: format!("failed to parse streamed response: {err}"),
        })
    } else {
        serde_json::from_str(&text).map_err(|err| ApiError::Http {
            status,
            body: format!("failed to parse JSON response: {err}"),
        })
    }
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
            input_images: vec![],
            model: Some("gpt-5.5".to_string()),
            image_model: Some("gpt-image-2".to_string()),
            format: OutputFormat::Png,
            size: Some("1024x1024".to_string()),
            quality: Some("low".to_string()),
            compression: Some(80),
            background: Some("opaque".to_string()),
            action: Some("generate".to_string()),
            tool_params: vec!["moderation=\"low\"".to_string()],
            instructions: None,
            tool_choice: ToolChoice::ImageGeneration,
            profile: None,
            codex_home: None,
            auth_store: crate::args::AuthStoreMode::Codex,
            auth_source: crate::args::AuthSource::Codex,
            base_url: None,
            no_stream: false,
            timeout_secs: 600,
            json: false,
            dry_run: false,
        }
    }

    #[test]
    fn request_body_forces_image_generation_tool() {
        let request = ImageRequest::from_cli(
            &cli(),
            "draw a test image".to_string(),
            "gpt-5.5".to_string(),
        )
        .unwrap();
        let body = request.to_body();

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], DEFAULT_INSTRUCTIONS);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tool_choice"]["type"], "image_generation");
        assert_eq!(body["tools"][0]["type"], "image_generation");
        assert_eq!(body["tools"][0]["model"], "gpt-image-2");
        assert_eq!(body["tools"][0]["output_format"], "png");
        assert_eq!(body["tools"][0]["output_compression"], 80);
        assert_eq!(body["tools"][0]["moderation"], "low");
    }

    #[test]
    fn request_body_uses_codex_auto_tool_choice_by_default() {
        let mut cli = cli();
        cli.tool_choice = ToolChoice::Auto;

        let request =
            ImageRequest::from_cli(&cli, "draw a test image".to_string(), "gpt-5.5".to_string())
                .unwrap();
        let body = request.to_body();

        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn request_body_includes_local_input_images_as_data_urls() {
        let temp_dir = tempfile::tempdir().unwrap();
        let image_path = temp_dir.path().join("input.png");
        std::fs::write(&image_path, b"png bytes").unwrap();
        let mut cli = cli();
        cli.input_images = vec![image_path];
        cli.action = Some("edit".to_string());

        let request =
            ImageRequest::from_cli(&cli, "edit this image".to_string(), "gpt-5.5".to_string())
                .unwrap();
        let body = request.to_body();
        let content = body["input"][0]["content"].as_array().unwrap();

        assert_eq!(content[0]["type"], "input_text");
        assert_eq!(content[1]["type"], "input_image");
        assert_eq!(
            content[1]["image_url"],
            "data:image/png;base64,cG5nIGJ5dGVz"
        );
        assert_eq!(body["tools"][0]["action"], "edit");
    }

    #[test]
    fn dry_run_body_redacts_local_input_image_data() {
        let temp_dir = tempfile::tempdir().unwrap();
        let image_path = temp_dir.path().join("input.jpg");
        std::fs::write(&image_path, b"jpeg bytes").unwrap();
        let mut cli = cli();
        cli.input_images = vec![image_path];

        let request =
            ImageRequest::from_cli(&cli, "edit this image".to_string(), "gpt-5.5".to_string())
                .unwrap();
        let body = request.to_redacted_body();
        let image_url = body["input"][0]["content"][1]["image_url"]
            .as_str()
            .unwrap();

        assert_eq!(image_url, "data:image/jpeg;base64,<redacted:16 chars>");
    }

    #[test]
    fn unsupported_local_input_image_extension_is_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let image_path = temp_dir.path().join("input.bmp");
        std::fs::write(&image_path, b"bmp bytes").unwrap();
        let mut cli = cli();
        cli.input_images = vec![image_path];

        let err =
            ImageRequest::from_cli(&cli, "edit this image".to_string(), "gpt-5.5".to_string())
                .unwrap_err();

        assert!(err.to_string().contains("unsupported input image format"));
    }
}
