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

    // Split "program arg1 arg2" on whitespace (no quote handling needed for typical $EDITOR values).
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("nano");
    let args: Vec<&str> = parts.collect();

    // Leave TUI
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    // Spawn editor and wait; capture result so TUI is always restored below.
    let status_res = tokio::process::Command::new(program)
        .args(&args)
        .arg(path)
        .status()
        .await;

    // Re-enter TUI — always attempt restoration even if the editor failed.
    // If restoration itself fails, combine with any prior editor error.
    let restore_res = enable_raw_mode()
        .and_then(|_| execute!(std::io::stdout(), EnterAlternateScreen));

    // Prefer the original editor error; surface restoration error only if no prior error.
    let status = match (status_res, restore_res) {
        (Err(editor_err), _) => return Err(editor_err.into()),
        (Ok(_), Err(restore_err)) => return Err(restore_err.into()),
        (Ok(status), Ok(_)) => status,
    };

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status: {}", status);
    }

    Ok(())
}
