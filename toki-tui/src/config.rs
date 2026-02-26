use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokiConfig {
    /// URL of the toki-api server. Defaults to the production instance.
    #[serde(default = "default_api_url")]
    pub api_url: String,
    /// Taskwarrior filter tokens prepended before `status:pending export`.
    /// Leave empty to show all pending tasks.
    #[serde(default)]
    pub task_filter: String,
    /// Prefix used when converting a git branch name to a time entry note
    /// when no conventional commit prefix or ticket number is found.
    #[serde(default = "default_git_prefix")]
    pub git_default_prefix: String,
}

fn default_api_url() -> String {
    "https://toki-api.spinit.se".to_string()
}

fn default_git_prefix() -> String {
    "Utveckling".to_string()
}

impl Default for TokiConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            task_filter: String::new(),
            git_default_prefix: default_git_prefix(),
        }
    }
}

impl TokiConfig {
    pub fn config_path() -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Cannot determine config directory")?
            .join("toki-tui")
            .join("config.toml"))
    }

    pub fn ensure_exists() -> Result<PathBuf> {
        let path = Self::config_path()?;
        if path.exists() {
            return Ok(path);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let raw = toml::to_string_pretty(&Self::default())
            .context("Failed to serialize default config")?;
        std::fs::write(&path, raw)
            .with_context(|| format!("Failed to write default config {}", path.display()))?;
        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        let settings = config::Config::builder()
            .set_default("api_url", default_api_url())?
            .set_default("task_filter", "")?
            .set_default("git_default_prefix", default_git_prefix())?
            .add_source(config::File::from(path.clone()).required(false))
            .add_source(
                config::Environment::with_prefix("TOKI_TUI")
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()
            .context("Failed to build TUI config")?;

        settings
            .try_deserialize::<Self>()
            .with_context(|| format!("Failed to parse config from {}", path.display()))
    }
}
