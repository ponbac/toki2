# Note View Git Context Features Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add git context awareness to the note editor view: show CWD/branch/last-commit, let user paste raw or parsed branch/commit as note text, and allow changing the working directory with tab completion.

**Architecture:** New `GitContext` struct in `app.rs` holds CWD + git data fetched via `std::process::Command`. New `git_mode` bool on `App` implements a two-key sequence (Ctrl+G then B/P/C/D). A `parse_branch()` pure function handles the branch-to-note transformation. UI changes are confined to `render_description_editor()` in `ui/mod.rs`. Key handling changes are confined to the `EditDescription` branch in `main.rs`.

**Tech Stack:** Rust, ratatui, crossterm, std::process::Command, std::fs::read_dir

---

### Task 1: Add `GitContext` struct and populate it in `App::new()`

**Files:**
- Modify: `src/app.rs`

**Step 1: Add the struct and fields**

In `src/app.rs`, after the `FocusedBox` enum (around line 45), add:

```rust
#[derive(Debug, Clone)]
pub struct GitContext {
    pub cwd: std::path::PathBuf,
    pub branch: Option<String>,
    pub last_commit: Option<String>,
}

impl GitContext {
    pub fn from_cwd(cwd: std::path::PathBuf) -> Self {
        let branch = Self::git_branch(&cwd);
        let last_commit = Self::git_last_commit(&cwd);
        Self { cwd, branch, last_commit }
    }

    fn git_branch(cwd: &std::path::Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["-C", cwd.to_str()?, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        }
    }

    fn git_last_commit(cwd: &std::path::Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["-C", cwd.to_str()?, "log", "-1", "--format=%s"])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        }
    }

    pub fn refresh(&mut self) {
        self.branch = Self::git_branch(&self.cwd);
        self.last_commit = Self::git_last_commit(&self.cwd);
    }
}
```

**Step 2: Add new fields to `App` struct**

In the `pub struct App { ... }` block (around line 80), add before the closing `}`:

```rust
    // Git context for note editor
    pub git_context: GitContext,
    pub git_mode: bool,
    pub cwd_input: Option<String>,       // Some(_) when changing directory
    pub cwd_completions: Vec<String>,    // Tab completion candidates
```

**Step 3: Initialize in `App::new()`**

In `App::new()`, add to the `Self { ... }` initializer:

```rust
            git_context: GitContext::from_cwd(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            git_mode: false,
            cwd_input: None,
            cwd_completions: Vec::new(),
```

**Step 4: Verify it compiles**

```bash
cargo check 2>&1
```
Expected: no errors (warnings ok).

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add GitContext struct to App state"
```

---

### Task 2: Add `parse_branch()` pure function with tests

**Files:**
- Create: `src/git.rs`
- Modify: `src/main.rs` (add `mod git;`)

**Step 1: Create `src/git.rs`**

```rust
/// Conventional commit type prefixes (lowercase).
const CONVENTIONAL_PREFIXES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];

/// Parse a git branch name into a human-readable note string.
///
/// Rules:
/// 1. If a standalone number exists (surrounded by `-` or `_`, or at start/end of a
///    slash-separated segment), extract it as `#NUM` prefix.
/// 2. Split on `/` to find an optional prefix.
///    - If prefix is a conventional commit type → `"prefix: rest"`
///    - If prefix exists but is NOT conventional → `"Utveckling: <original branch>"`
///    - No `/` → treat entire branch as the "rest" part
/// 3. Replace remaining `-` and `_` with spaces; trim.
/// 4. If no number and no conventional prefix → `"Utveckling: <branch>"`
pub fn parse_branch(branch: &str) -> String {
    // Split into prefix and rest on first `/`
    let (slash_prefix, rest_after_slash) = if let Some(idx) = branch.find('/') {
        let p = &branch[..idx];
        let r = &branch[idx + 1..];
        (Some(p), r)
    } else {
        (None, branch)
    };

    // Extract standalone number from the rest part.
    // A "standalone" number is one surrounded by `-`, `_`, or at segment start/end.
    let (number, rest_without_number) = extract_number(rest_after_slash);

    // Determine the conventional prefix (if any)
    let is_conventional = slash_prefix
        .map(|p| CONVENTIONAL_PREFIXES.contains(&p.to_lowercase().as_str()))
        .unwrap_or(false);

    // Humanize: replace `-` and `_` with space, trim
    let humanized = rest_without_number
        .replace('-', " ")
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    match (number, slash_prefix, is_conventional) {
        // Has number + conventional prefix  →  "#NUM - prefix: rest"
        (Some(n), Some(p), true) => format!("#{} - {}: {}", n, p.to_lowercase(), humanized),

        // Has number + no prefix            →  "#NUM - rest"
        (Some(n), None, _) => format!("#{} - {}", n, humanized),

        // Has number + non-conventional prefix  →  "#NUM - Utveckling: branch"
        (Some(n), Some(_), false) => format!("#{} - Utveckling: {}", n, branch_without_prefix(branch)),

        // No number + conventional prefix   →  "prefix: rest"
        (None, Some(p), true) => format!("{}: {}", p.to_lowercase(), humanized),

        // No number + non-conventional prefix OR no prefix at all with no number  →  "Utveckling: branch"
        (None, Some(_), false) => format!("Utveckling: {}", branch),
        (None, None, _) => format!("Utveckling: {}", branch),
    }
}

/// Returns the slash-prefix stripped part of a branch (everything after the first `/`).
fn branch_without_prefix(branch: &str) -> &str {
    if let Some(idx) = branch.find('/') {
        &branch[idx + 1..]
    } else {
        branch
    }
}

/// Extract a standalone number from a string segment.
/// "Standalone" = surrounded by `-`/`_` or at the boundary of the string.
/// Returns (Some(number_string), rest_with_number_and_separators_removed) or (None, original).
fn extract_number(s: &str) -> (Option<String>, String) {
    // Find all digit runs and check if they're standalone
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            // Find end of digit run
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let end = i; // exclusive

            // Check boundaries
            let left_ok = start == 0 || chars[start - 1] == '-' || chars[start - 1] == '_';
            let right_ok = end == chars.len() || chars[end] == '-' || chars[end] == '_';

            if left_ok && right_ok {
                let number: String = chars[start..end].iter().collect();
                // Remove the number and its surrounding separator(s)
                let mut rest = String::new();
                // chars before separator+number
                if start > 0 {
                    // include everything before the left separator
                    rest.push_str(&chars[..start - 1].iter().collect::<String>());
                }
                // chars after number (skip right separator if present)
                let right_start = if end < chars.len() { end + 1 } else { end };
                rest.push_str(&chars[right_start..].iter().collect::<String>());
                return (Some(number), rest);
            }
        } else {
            i += 1;
        }
    }
    (None, s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_with_number() {
        assert_eq!(parse_branch("fix/8322-styling-adjustments"), "#8322 - fix: styling adjustments");
    }

    #[test]
    fn test_number_only_no_prefix() {
        assert_eq!(parse_branch("8322-mybranch"), "#8322 - mybranch");
    }

    #[test]
    fn test_feature_with_number_underscore() {
        assert_eq!(parse_branch("feature/8322_tests"), "#8322 - feat: tests");
        // Note: "feature" is a conventional alias for "feat" — handle below or use "feature" directly.
        // Adjust expectation: the spec says "feature/8322_tests" → "#8322 - feature: tests"
        // so we keep the prefix as-is (lowercase).
    }

    #[test]
    fn test_main_branch() {
        assert_eq!(parse_branch("main"), "Utveckling: main");
    }

    #[test]
    fn test_non_conventional_prefix() {
        assert_eq!(parse_branch("branding/testbageriet"), "Utveckling: branding/testbageriet");
    }

    #[test]
    fn test_number_not_standalone() {
        assert_eq!(parse_branch("test/feature2"), "test: feature2");
    }
}
```

**Important note on `feature` prefix:** The spec examples show `feature/8322_tests` → `"#8322 - feature: tests"` — so `feature` should be in `CONVENTIONAL_PREFIXES`. Add it.

**Step 2: Register module in `main.rs`**

Add `mod git;` near the top of `src/main.rs` alongside the other `mod` declarations.

**Step 3: Run the tests**

```bash
cargo test git 2>&1
```

Expected: all tests in `git.rs` pass. Fix any failures (likely the `feature` vs `feat` alias — keep `feature` in the list verbatim).

**Step 4: Commit**

```bash
git add src/git.rs src/main.rs
git commit -m "feat: add parse_branch() with tests"
```

---

### Task 3: Add git-related methods to `App`

**Files:**
- Modify: `src/app.rs`

Add these methods to `impl App`:

```rust
/// Enter git mode (waiting for second key after Ctrl+G).
pub fn enter_git_mode(&mut self) {
    self.git_mode = true;
}

/// Exit git mode without action.
pub fn exit_git_mode(&mut self) {
    self.git_mode = false;
}

/// Paste raw branch name into description_input.
pub fn paste_git_branch_raw(&mut self) {
    self.git_mode = false;
    if let Some(branch) = &self.git_context.branch.clone() {
        self.description_input = branch.clone();
    }
}

/// Paste parsed branch name into description_input.
pub fn paste_git_branch_parsed(&mut self) {
    self.git_mode = false;
    if let Some(branch) = &self.git_context.branch.clone() {
        self.description_input = crate::git::parse_branch(branch);
    }
}

/// Paste last commit message into description_input.
pub fn paste_git_last_commit(&mut self) {
    self.git_mode = false;
    if let Some(commit) = &self.git_context.last_commit.clone() {
        self.description_input = commit.clone();
    }
}

/// Begin CWD change mode. Pre-fill with current cwd string.
pub fn begin_cwd_change(&mut self) {
    self.git_mode = false;
    self.cwd_input = Some(self.git_context.cwd.to_string_lossy().to_string());
    self.cwd_completions = Vec::new();
}

/// Cancel CWD change mode.
pub fn cancel_cwd_change(&mut self) {
    self.cwd_input = None;
    self.cwd_completions = Vec::new();
}

/// Confirm CWD change. Returns Err if path doesn't exist.
pub fn confirm_cwd_change(&mut self) -> Result<(), String> {
    let input = self.cwd_input.take().unwrap_or_default();
    self.cwd_completions = Vec::new();
    let path = std::path::PathBuf::from(&input);
    if path.is_dir() {
        self.git_context = crate::app::GitContext::from_cwd(path);
        Ok(())
    } else {
        self.cwd_input = Some(input);
        Err(format!("Not a directory: {}", path.display()))
    }
}

/// Tab-complete the current cwd_input. Fills cwd_completions with matches,
/// and completes to longest common prefix if there are matches.
pub fn cwd_tab_complete(&mut self) {
    let input = match &self.cwd_input {
        Some(s) => s.clone(),
        None => return,
    };

    let path = std::path::Path::new(&input);
    let (dir, prefix) = if input.ends_with('/') || input.ends_with(std::path::MAIN_SEPARATOR) {
        (path, "")
    } else {
        (
            path.parent().unwrap_or(std::path::Path::new(".")),
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""),
        )
    };

    let mut matches: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(prefix) {
                        let full = format!(
                            "{}/{}",
                            dir.to_string_lossy().trim_end_matches('/'),
                            name
                        );
                        matches.push(full);
                    }
                }
            }
        }
    }
    matches.sort();

    if matches.len() == 1 {
        self.cwd_input = Some(format!("{}/", matches[0]));
        self.cwd_completions = Vec::new();
    } else if matches.len() > 1 {
        // Find longest common prefix
        let lcp = longest_common_prefix(&matches);
        self.cwd_input = Some(lcp);
        self.cwd_completions = matches;
    }
    // no matches: do nothing
}

/// Append a char to cwd_input.
pub fn cwd_input_char(&mut self, c: char) {
    if let Some(s) = &mut self.cwd_input {
        s.push(c);
        self.cwd_completions.clear();
    }
}

/// Backspace in cwd_input.
pub fn cwd_input_backspace(&mut self) {
    if let Some(s) = &mut self.cwd_input {
        s.pop();
        self.cwd_completions.clear();
    }
}
```

Also add a free function at the bottom of `app.rs` (outside `impl App`):

```rust
fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.chars().zip(s.chars()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    first[..len].to_string()
}
```

**Verify it compiles:**

```bash
cargo check 2>&1
```

**Commit:**

```bash
git add src/app.rs
git commit -m "feat: add git mode and CWD change methods to App"
```

---

### Task 4: Handle git key events in `main.rs`

**Files:**
- Modify: `src/main.rs`

Find the `app::View::EditDescription => { ... }` match arm (around line 187). The current structure is:

```rust
app::View::EditDescription => {
    match key.code {
        KeyCode::Char('x') | ... => { ... }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input_char(c);
        }
        ...
    }
}
```

Replace this arm with the expanded version below. The key insight is that when `app.cwd_input.is_some()`, all typing goes to the CWD input instead of the note. When `app.git_mode` is true, the next key selects a git action.

```rust
app::View::EditDescription => {
    let was_in_edit_mode = app.is_in_edit_mode();

    // CWD change mode takes priority
    if app.cwd_input.is_some() {
        match key.code {
            KeyCode::Esc => app.cancel_cwd_change(),
            KeyCode::Enter => {
                if let Err(e) = app.confirm_cwd_change() {
                    app.status_message = Some((e, std::time::Instant::now()));
                }
            }
            KeyCode::Tab => app.cwd_tab_complete(),
            KeyCode::Backspace => app.cwd_input_backspace(),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.cwd_input_char(c);
            }
            _ => {}
        }
    } else if app.git_mode {
        // Second key of Ctrl+G sequence
        match key.code {
            KeyCode::Char('b') | KeyCode::Char('B') => app.paste_git_branch_raw(),
            KeyCode::Char('p') | KeyCode::Char('P') => app.paste_git_branch_parsed(),
            KeyCode::Char('c') | KeyCode::Char('C') => app.paste_git_last_commit(),
            KeyCode::Char('d') | KeyCode::Char('D') => app.begin_cwd_change(),
            _ => app.exit_git_mode(), // any other key cancels git mode
        }
    } else {
        match key.code {
            KeyCode::Char('x') | KeyCode::Char('X')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.description_input.clear();
            }
            KeyCode::Char('g') | KeyCode::Char('G')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.enter_git_mode();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.input_char(c);
            }
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Enter => {
                if was_in_edit_mode {
                    app.update_edit_state_note(app.description_input.clone());
                    if let Some(saved_note) = app.saved_timer_note.take() {
                        app.description_input = saved_note;
                    }
                    let return_view = app.get_return_view_from_edit();
                    app.navigate_to(return_view);
                    if return_view == app::View::Timer {
                        app.focused_box = app::FocusedBox::Today;
                    }
                } else {
                    app.confirm_description();
                }
            }
            KeyCode::Esc => {
                if was_in_edit_mode {
                    if let Some(saved_note) = app.saved_timer_note.take() {
                        app.description_input = saved_note;
                    }
                    let return_view = app.get_return_view_from_edit();
                    app.navigate_to(return_view);
                    if return_view == app::View::Timer {
                        app.focused_box = app::FocusedBox::Today;
                    }
                } else {
                    app.cancel_selection();
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.quit();
            }
            _ => {}
        }
    }
}
```

**Verify:**

```bash
cargo check 2>&1
```

**Commit:**

```bash
git add src/main.rs
git commit -m "feat: handle git mode key events in EditDescription"
```

---

### Task 5: Update `render_description_editor()` in `ui/mod.rs`

**Files:**
- Modify: `src/ui/mod.rs`

**Step 1: Update the layout**

Replace the existing layout constraints in `render_description_editor()` (line ~980):

```rust
// Old:
.constraints([
    Constraint::Length(3), // Header
    Constraint::Length(3), // Input field
    Constraint::Min(0),    // Spacer
    Constraint::Length(3), // Controls
])

// New:
.constraints([
    Constraint::Length(3), // Header
    Constraint::Length(3), // Input field or CWD input
    Constraint::Length(5), // Git context panel
    Constraint::Min(0),    // Spacer
    Constraint::Length(3), // Controls
])
```

All references to `chunks[2]` (spacer) and `chunks[3]` (controls) must be updated to `chunks[3]` and `chunks[4]`.

**Step 2: Replace the input block**

After rendering the header (`chunks[0]`), render either the CWD input or the note input in `chunks[1]`:

```rust
// Input field (note or CWD change)
if let Some(cwd_input) = &app.cwd_input {
    let completions_hint = if app.cwd_completions.is_empty() {
        String::new()
    } else {
        format!("  [{}]", app.cwd_completions.join("  "))
    };
    let input_text = format!("{}█{}", cwd_input, completions_hint);
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Change Directory ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(input, chunks[1]);
} else {
    let input_text = format!("{}█", app.description_input);
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Note ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(input, chunks[1]);
}
```

**Step 3: Render the git context panel in `chunks[2]`**

```rust
// Git context panel
let has_git = app.git_context.branch.is_some();
let git_color = if has_git { Color::White } else { Color::DarkGray };
let muted = Color::DarkGray;

let cwd_str = app.git_context.cwd.to_string_lossy().to_string();
let branch_str = app.git_context.branch.as_deref().unwrap_or("(no git repo)");
let commit_str = app.git_context.last_commit.as_deref().unwrap_or("(none)");

let git_lines = vec![
    Line::from(vec![
        Span::styled("Dir:    ", Style::default().fg(muted)),
        Span::styled(cwd_str, Style::default().fg(Color::Cyan)),
    ]),
    Line::from(vec![
        Span::styled("Branch: ", Style::default().fg(muted)),
        Span::styled(branch_str, Style::default().fg(git_color)),
    ]),
    Line::from(vec![
        Span::styled("Commit: ", Style::default().fg(muted)),
        Span::styled(commit_str, Style::default().fg(git_color)),
    ]),
];

let git_panel = Paragraph::new(git_lines)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Git ")
            .padding(Padding::horizontal(1)),
    );
frame.render_widget(git_panel, chunks[2]);
```

**Step 4: Update controls in `chunks[4]`**

Replace the existing controls block at `chunks[3]` (now `chunks[4]`) with context-sensitive controls:

```rust
// Controls (context-sensitive)
let controls_text: Vec<Span> = if app.cwd_input.is_some() {
    vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": path  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": complete  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": confirm  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": cancel"),
    ]
} else if app.git_mode {
    let git_key_style = if has_git {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    vec![
        Span::styled("[git mode] ", Style::default().fg(Color::Cyan)),
        Span::styled("B", git_key_style),
        Span::raw(": raw branch  "),
        Span::styled("P", git_key_style),
        Span::raw(": parsed branch  "),
        Span::styled("C", git_key_style),
        Span::raw(": last commit  "),
        Span::styled("D", Style::default().fg(Color::Yellow)),
        Span::raw(": change dir  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": cancel"),
    ]
} else {
    let git_key_style = if has_git {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": edit  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": clear  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": confirm  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": cancel  "),
        Span::styled("Ctrl+G", git_key_style),
        Span::raw(": git…"),
    ]
};

let controls = Paragraph::new(Line::from(controls_text))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).title(" Controls "));
frame.render_widget(controls, chunks[4]);
```

**Verify:**

```bash
cargo check 2>&1
```

**Commit:**

```bash
git add src/ui/mod.rs
git commit -m "feat: update note editor UI with git context panel and dynamic controls"
```

---

### Task 6: Fix `feature` in `CONVENTIONAL_PREFIXES` and verify all tests + build

The spec example: `feature/8322_tests` → `"#8322 - feature: tests"`.
Make sure `"feature"` is in `CONVENTIONAL_PREFIXES` in `src/git.rs` (not `"feat"` only).

**Run tests:**

```bash
cargo test 2>&1
```

Expected: all tests pass.

**Run full check:**

```bash
cargo check 2>&1
```

**Final commit if any fixups needed:**

```bash
git add -A
git commit -m "fix: ensure all branch parsing tests pass"
```

---

### Summary of all new files / changed files

| File | Change |
|------|--------|
| `src/app.rs` | New `GitContext` struct; new fields on `App`; new methods |
| `src/git.rs` | New file: `parse_branch()` + `extract_number()` + tests |
| `src/main.rs` | Add `mod git;`; expand `EditDescription` key handler |
| `src/ui/mod.rs` | New layout chunk; git panel; CWD input; dynamic controls |
