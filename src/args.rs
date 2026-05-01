use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use clap::ArgGroup;
use clap::Parser;
use clap::ValueEnum;

#[derive(Debug, Parser)]
#[command(name = "codex-imagegen")]
#[command(
    version,
    about = "Generate one image through Codex hosted image_generation."
)]
#[command(group(
    ArgGroup::new("prompt_source")
        .required(true)
        .args(["prompt", "prompt_file", "prompt_arg"])
))]
pub struct Cli {
    #[arg(short, long, value_name = "TEXT")]
    pub prompt: Option<String>,

    #[arg(long, value_name = "PATH")]
    pub prompt_file: Option<PathBuf>,

    #[arg(value_name = "PROMPT")]
    pub prompt_arg: Option<String>,

    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "gpt-5.5")]
    pub model: String,

    #[arg(long, value_name = "MODEL")]
    pub image_model: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Png)]
    pub format: OutputFormat,

    #[arg(long, value_name = "SIZE")]
    pub size: Option<String>,

    #[arg(long, value_name = "QUALITY")]
    pub quality: Option<String>,

    #[arg(long, value_name = "0-100")]
    pub compression: Option<u8>,

    #[arg(long, value_name = "BACKGROUND")]
    pub background: Option<String>,

    #[arg(long, value_name = "ACTION")]
    pub action: Option<String>,

    #[arg(long = "tool-param", value_name = "KEY=JSON_OR_TEXT")]
    pub tool_params: Vec<String>,

    #[arg(long, value_name = "PATH")]
    pub codex_home: Option<PathBuf>,

    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,

    #[arg(long, default_value_t = 600)]
    pub timeout_secs: u64,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    Png,
    Jpeg,
    Webp,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Cli {
    pub fn prompt_text(&self) -> Result<String> {
        let prompt = match (&self.prompt, &self.prompt_file, &self.prompt_arg) {
            (Some(text), None, None) => text.clone(),
            (None, Some(path), None) => fs::read_to_string(path)?,
            (None, None, Some(text)) => text.clone(),
            _ => bail!("provide exactly one prompt source"),
        };

        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            bail!("prompt is empty");
        }
        Ok(trimmed.to_string())
    }
}
