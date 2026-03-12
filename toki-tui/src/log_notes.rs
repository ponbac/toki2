use std::path::PathBuf;

const TAG_PREFIX: &str = "  [log:";
const TAG_SUFFIX: &str = "]";

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

/// Generates a random 6-character lowercase hex ID.
pub fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Simple deterministic-enough ID from timestamp nanos XOR'd with secs.
    // No external deps needed. 6 hex chars = 16M values, plenty for thousands of logs.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hash = (secs ^ (nanos as u64)).wrapping_mul(0x9e3779b97f4a7c15);
    format!("{:06x}", hash & 0xffffff)
}

/// Extracts the log ID from a note string, if the tag is present.
/// e.g. "Fixed auth bug  [log:a3f8b2]" → Some("a3f8b2")
pub fn extract_id(note: &str) -> Option<&str> {
    let pos = note.find(TAG_PREFIX)?;
    let after = &note[pos + TAG_PREFIX.len()..];
    // ID is exactly 6 hex chars followed by ']'
    if after.len() >= 7
        && after[..6].chars().all(|c| c.is_ascii_hexdigit())
        && after.as_bytes()[6] == b']'
    {
        Some(&after[..6])
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
    format!("{}{}{}{}", note.trim_end(), TAG_PREFIX, id, TAG_SUFFIX)
}

/// Writes the initial log file with YAML frontmatter.
/// Only `id` and `date` are stored — project/activity/note are not tracked
/// since they can change after creation and would quickly go stale.
pub fn create_log_file(id: &str, date: &str) -> anyhow::Result<PathBuf> {
    let path = log_path(id)?;
    if !path.exists() {
        let content = format!("---\nid: {}\ndate: {}\n---\n\n", id, date);
        std::fs::write(&path, content)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_id_present() {
        let note = "Fixed auth bug  [log:a3f8b2]";
        assert_eq!(extract_id(note), Some("a3f8b2"));
    }

    #[test]
    fn test_extract_id_absent() {
        assert_eq!(extract_id("No log here"), None);
    }

    #[test]
    fn test_strip_tag() {
        let note = "Fixed auth bug  [log:a3f8b2]";
        assert_eq!(strip_tag(note), "Fixed auth bug");
    }

    #[test]
    fn test_strip_tag_no_tag() {
        assert_eq!(strip_tag("Plain note"), "Plain note");
    }

    #[test]
    fn test_append_tag() {
        let result = append_tag("Fixed auth bug", "a3f8b2");
        assert_eq!(result, "Fixed auth bug  [log:a3f8b2]");
    }

    #[test]
    fn test_append_tag_trims_trailing_space() {
        let result = append_tag("Fixed auth bug   ", "a3f8b2");
        assert_eq!(result, "Fixed auth bug  [log:a3f8b2]");
    }

    #[test]
    fn test_generate_id_length() {
        let id = generate_id();
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
