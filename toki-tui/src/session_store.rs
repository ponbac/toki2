use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::{io::Write, os::unix::fs::OpenOptionsExt};

fn root_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("Cannot determine config directory")?
        .join("toki-tui"))
}

fn secure_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?
            .write_all(content.as_bytes())?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content)?;
    }

    Ok(())
}

pub fn session_path() -> Result<PathBuf> {
    Ok(root_path()?.join("session"))
}

pub fn mt_cookies_path() -> Result<PathBuf> {
    Ok(root_path()?.join("mt_cookies"))
}

pub fn load_session() -> Result<Option<String>> {
    let path = session_path()?;
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

pub fn save_session(session_id: &str) -> Result<()> {
    let path = session_path()?;
    secure_write(path.as_path(), session_id)
}

pub fn clear_session() -> Result<()> {
    let path = session_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn load_mt_cookies() -> Result<Vec<(String, String)>> {
    let path = mt_cookies_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let raw = std::fs::read_to_string(path).context("Failed to read mt_cookies")?;
    Ok(raw
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
        .collect())
}

pub fn save_mt_cookies(cookies: &[(String, String)]) -> Result<()> {
    let path = mt_cookies_path()?;
    let content = cookies
        .iter()
        .map(|(name, value)| format!("{}={}", name, value))
        .collect::<Vec<_>>()
        .join("\n");
    secure_write(path.as_path(), &content)
}

pub fn clear_mt_cookies() -> Result<()> {
    let path = mt_cookies_path()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
