use std::path::PathBuf;

const TAG_PREFIX: &str = "  \u{00B7}log:"; // "  ·log:"

/// Returns the log storage directory: ~/.local/share/toki-tui/logs/
pub fn log_dir() -> anyhow::Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine local data directory"))?
        .join("toki-tui")
        .join("logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the path for a given log ID.
pub fn log_path(id: &str) -> anyhow::Result<PathBuf> {
    Ok(log_dir()?.join(format!("{}.md", id)))
}

/// Generates a random 8-character lowercase hex ID.
pub fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Simple deterministic-enough ID from timestamp nanos XOR'd with a counter
    // No external deps needed.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{:08x}",
        (secs ^ (nanos as u64)).wrapping_mul(0x9e3779b97f4a7c15)
    )
}

/// Extracts the log ID from a note string, if the tag is present.
/// e.g. "Fixed auth bug  ·log:a3f8b2c1" → Some("a3f8b2c1")
pub fn extract_id(note: &str) -> Option<&str> {
    let pos = note.find(TAG_PREFIX)?;
    let after = &note[pos + TAG_PREFIX.len()..];
    // ID is exactly 8 hex chars
    if after.len() >= 8 && after[..8].chars().all(|c| c.is_ascii_hexdigit()) {
        Some(&after[..8])
    } else {
        None
    }
}

/// Strips the log tag from a note for display purposes.
pub fn strip_tag(note: &str) -> &str {
    if let Some(pos) = note.find(TAG_PREFIX) {
        note[..pos].trim_end()
    } else {
        note
    }
}

/// Appends a log tag to a note string (returns new String).
pub fn append_tag(note: &str, id: &str) -> String {
    format!("{}{}{}", note.trim_end(), TAG_PREFIX, id)
}

/// Writes the initial log file with YAML frontmatter.
pub fn create_log_file(
    id: &str,
    date: &str,
    project: &str,
    activity: &str,
    note_summary: &str,
) -> anyhow::Result<PathBuf> {
    let path = log_path(id)?;
    if !path.exists() {
        let content = format!(
            "---\nid: {}\ndate: {}\nproject: {}\nactivity: {}\nnote: {}\n---\n\n",
            id, date, project, activity, note_summary
        );
        std::fs::write(&path, content)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_id_present() {
        let note = "Fixed auth bug  \u{00B7}log:a3f8b2c1";
        assert_eq!(extract_id(note), Some("a3f8b2c1"));
    }

    #[test]
    fn test_extract_id_absent() {
        assert_eq!(extract_id("No log here"), None);
    }

    #[test]
    fn test_strip_tag() {
        let note = "Fixed auth bug  \u{00B7}log:a3f8b2c1";
        assert_eq!(strip_tag(note), "Fixed auth bug");
    }

    #[test]
    fn test_strip_tag_no_tag() {
        assert_eq!(strip_tag("Plain note"), "Plain note");
    }

    #[test]
    fn test_append_tag() {
        let result = append_tag("Fixed auth bug", "a3f8b2c1");
        assert_eq!(result, "Fixed auth bug  \u{00B7}log:a3f8b2c1");
    }

    #[test]
    fn test_append_tag_trims_trailing_space() {
        let result = append_tag("Fixed auth bug   ", "a3f8b2c1");
        assert_eq!(result, "Fixed auth bug  \u{00B7}log:a3f8b2c1");
    }
}
