use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::Utc;
use serde::Serialize;

#[derive(Serialize)]
pub struct GenerationReport {
    pub response_id: Option<String>,
    pub image_id: Option<String>,
    pub revised_prompt: Option<String>,
    pub output_path: PathBuf,
    pub format: String,
}

pub fn resolve_output_path(output: Option<&Path>, format: &str) -> Result<PathBuf> {
    let extension = extension_for_format(format)?;
    match output {
        Some(path) if path.is_dir() => Ok(path.join(default_file_name(extension))),
        Some(path) => Ok(path.to_path_buf()),
        None => Ok(PathBuf::from("generated").join(default_file_name(extension))),
    }
}

pub async fn write_base64_image(path: &Path, result: &str) -> Result<()> {
    let bytes = BASE64_STANDARD
        .decode(result.trim().as_bytes())
        .context("invalid base64 image payload")?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

fn default_file_name(extension: &str) -> String {
    format!("image-{}.{}", Utc::now().format("%Y%m%d-%H%M%S"), extension)
}

fn extension_for_format(format: &str) -> Result<&'static str> {
    match format {
        "png" => Ok("png"),
        "jpeg" | "jpg" => Ok("jpg"),
        "webp" => Ok("webp"),
        other => Err(anyhow!(
            "unsupported output format for file extension: {other}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_base64_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.bin");

        write_base64_image(&path, "aGVsbG8=").await.unwrap();

        let written = tokio::fs::read(path).await.unwrap();
        assert_eq!(written, b"hello");
    }

    #[test]
    fn resolves_default_output_path() {
        let path = resolve_output_path(None, "png").unwrap();
        assert!(path.starts_with("generated"));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
    }
}
