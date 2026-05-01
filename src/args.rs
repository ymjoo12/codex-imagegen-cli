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
    #[arg(long, value_name = "TEXT")]
    pub prompt: Option<String>,

    #[arg(long, value_name = "PATH")]
    pub prompt_file: Option<PathBuf>,

    #[arg(value_name = "PROMPT")]
    pub prompt_arg: Option<String>,

    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    #[arg(long, short = 'm')]
    pub model: Option<String>,

    #[arg(long = "profile", short = 'p', value_name = "CONFIG_PROFILE")]
    pub profile: Option<String>,

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

    #[arg(long, value_name = "TEXT")]
    pub instructions: Option<String>,

    #[arg(long, value_enum, default_value_t = ToolChoice::Auto)]
    pub tool_choice: ToolChoice,

    #[arg(long, value_name = "PATH")]
    pub codex_home: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = AuthStoreMode::Codex)]
    pub auth_store: AuthStoreMode,

    #[arg(long, value_enum, default_value_t = AuthSource::Codex)]
    pub auth_source: AuthSource,

    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,

    #[arg(long)]
    pub no_stream: bool,

    #[arg(long, default_value_t = 600)]
    pub timeout_secs: u64,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AuthSource {
    /// Match Codex provider precedence: provider auth, then managed auth.
    Codex,
    /// Use the selected Codex model provider only.
    Provider,
    /// Skip model-provider auth and use env/API-key/ChatGPT credential stores.
    Managed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum AuthStoreMode {
    /// Match Codex's cli_auth_credentials_store setting from config.toml.
    Codex,
    /// Try the OS keyring first, then fall back to auth.json.
    Auto,
    /// Read and write CODEX_HOME/auth.json.
    File,
    /// Read and write the OS keyring entry used by Codex.
    Keyring,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ToolChoice {
    /// Match Codex CLI Responses requests.
    Auto,
    /// Force the hosted image_generation tool.
    #[value(name = "image-generation", alias = "image_generation")]
    ImageGeneration,
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
