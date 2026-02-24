use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokiConfig {
    /// Base URL of the toki-api server, e.g. "http://localhost:8080"
    #[serde(default = "default_api_url")]
    pub api_url: String,
}

fn default_api_url() -> String {
    "http://localhost:8080".to_string()
}

impl Default for TokiConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
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

    pub fn session_path() -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Cannot determine config directory")?
            .join("toki-tui")
            .join("session"))
    }

    /// Load config from disk. Returns default config if file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        let config: Self = toml::from_str(&raw)
            .with_context(|| format!("Failed to parse config at {}", path.display()))?;
        Ok(config)
    }

    /// Save config to disk, creating parent directories as needed.
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(&path, raw)?;
        Ok(())
    }

    /// Load the saved session ID from disk. Returns None if not logged in.
    pub fn load_session() -> Result<Option<String>> {
        let path = Self::session_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let session = std::fs::read_to_string(&path).context("Failed to read session file")?;
        let session = session.trim().to_string();
        if session.is_empty() {
            return Ok(None);
        }
        Ok(Some(session))
    }

    /// Save the session ID to disk.
    pub fn save_session(session_id: &str) -> Result<()> {
        let path = Self::session_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, session_id)?;
        Ok(())
    }

    /// Delete the saved session (logout).
    pub fn clear_session() -> Result<()> {
        let path = Self::session_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn mt_cookies_path() -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Cannot determine config directory")?
            .join("toki-tui")
            .join("mt_cookies"))
    }

    /// Load saved Milltime cookies from disk. Returns empty vec if file doesn't exist.
    pub fn load_mt_cookies() -> Result<Vec<(String, String)>> {
        let path = Self::mt_cookies_path()?;
        if !path.exists() {
            return Ok(vec![]);
        }
        let raw = std::fs::read_to_string(&path).context("Failed to read mt_cookies")?;
        let cookies = raw
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, '=');
                let name = parts.next()?.trim().to_string();
                let value = parts.next()?.trim().to_string();
                if name.is_empty() {
                    None
                } else {
                    Some((name, value))
                }
            })
            .collect();
        Ok(cookies)
    }

    /// Save Milltime cookies to disk.
    pub fn save_mt_cookies(cookies: &[(String, String)]) -> Result<()> {
        let path = Self::mt_cookies_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = cookies
            .iter()
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Delete saved Milltime cookies.
    pub fn clear_mt_cookies() -> Result<()> {
        let path = Self::mt_cookies_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}
