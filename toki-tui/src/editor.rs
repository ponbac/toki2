use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::path::Path;

/// Suspend the TUI, open $EDITOR on `path`, then restore the TUI.
/// Returns Ok(()) on success. The file is NOT read back here — caller reads it.
pub async fn open_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "nano".to_string());

    // Leave TUI
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    // Spawn editor and wait; capture result so TUI is always restored below
    let status_res = tokio::process::Command::new(&editor)
        .arg(path)
        .status()
        .await;

    // Re-enter TUI (always, even if the editor failed to launch)
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;

    let status = status_res?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status: {}", status);
    }

    Ok(())
}
