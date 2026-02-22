# Cursor Inputs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add block-cursor display to time inputs and mid-string cursor navigation (left/right/Home/End) to all text inputs.

**Architecture:** Introduce a `TextInput` struct in `app.rs` encapsulating a `String` + `cursor: usize`. Replace the six bare `String` text-input fields with `TextInput`. The time inputs (`start_time_input`, `end_time_input`) remain plain `String`s — their cursor is purely a rendering concern derived from `len()`. All cursor rendering stays in `ui/mod.rs`.

**Tech Stack:** Rust, ratatui. No new dependencies.

---

## Task 1: Add `TextInput` struct to `app.rs`

**Files:**
- Modify: `toki-tui/src/app.rs`

All text inputs (note/description, project search, activity search, CWD, and `EntryEditState::note`) will use this struct. Time inputs stay as plain `String`.

**Step 1: Add the struct above `EntryEditState`** (around line 123):

```rust
/// A text input with mid-string cursor support.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextInput {
    pub value: String,
    pub cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_str(s: &str) -> Self {
        Self { value: s.to_string(), cursor: s.len() }
    }

    /// Insert a character at the cursor position.
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character immediately before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 { return; }
        // Step back one char boundary
        let new_cursor = self.prev_boundary(self.cursor);
        self.value.drain(new_cursor..self.cursor);
        self.cursor = new_cursor;
    }

    /// Move cursor one char to the left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_boundary(self.cursor);
        }
    }

    /// Move cursor one char to the right.
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor = self.next_boundary(self.cursor);
        }
    }

    pub fn home(&mut self) { self.cursor = 0; }
    pub fn end(&mut self)  { self.cursor = self.value.len(); }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Returns the string split at the cursor: (before, after).
    /// Use for rendering: `before + "█" + after`.
    pub fn split_at_cursor(&self) -> (&str, &str) {
        (&self.value[..self.cursor], &self.value[self.cursor..])
    }

    // --- helpers ---
    fn prev_boundary(&self, pos: usize) -> usize {
        let mut p = pos;
        loop {
            p -= 1;
            if self.value.is_char_boundary(p) { return p; }
        }
    }
    fn next_boundary(&self, pos: usize) -> usize {
        let mut p = pos + 1;
        while p <= self.value.len() && !self.value.is_char_boundary(p) { p += 1; }
        p
    }
}
```

**Step 2: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```
Expected: compiles (no usages changed yet).

---

## Task 2: Migrate `EntryEditState::note` to `TextInput`

**Files:**
- Modify: `toki-tui/src/app.rs`

**Step 1: Change the field** (line ~143):

```rust
// Before:
pub note: String,
// After:
pub note: TextInput,
```

**Step 2: Update `create_edit_state()` initialiser** (around line 650–692). Find where `note` is set:

```rust
// Before:
note: entry.note.clone().unwrap_or_default(),
// After:
note: TextInput::from_str(&entry.note.clone().unwrap_or_default()),
```

**Step 3: Update `entry_edit_input_char()` note arm** (line ~821):

```rust
// Before:
EntryEditField::Note => { state.note.push(c); }
// After:
EntryEditField::Note => { state.note.insert(c); }
```

**Step 4: Update `entry_edit_backspace()` note arm** (line ~850):

```rust
// Before:
EntryEditField::Note => { state.note.pop(); }
// After:
EntryEditField::Note => { state.note.backspace(); }
```

**Step 5: Update `entry_edit_move_cursor()` — add new method after `entry_edit_backspace()`:**

```rust
/// Move cursor left/right in a text field (Note only; time fields have no cursor).
pub fn entry_edit_move_cursor(&mut self, left: bool) {
    let apply = |state: &mut EntryEditState| {
        if state.focused_field == EntryEditField::Note {
            if left { state.note.move_left(); } else { state.note.move_right(); }
        }
    };
    if let Some(s) = &mut self.this_week_edit_state { apply(s); }
    if let Some(s) = &mut self.history_edit_state   { apply(s); }
}

pub fn entry_edit_cursor_home_end(&mut self, home: bool) {
    let apply = |state: &mut EntryEditState| {
        if state.focused_field == EntryEditField::Note {
            if home { state.note.home(); } else { state.note.end(); }
        }
    };
    if let Some(s) = &mut self.this_week_edit_state { apply(s); }
    if let Some(s) = &mut self.history_edit_state   { apply(s); }
}
```

**Step 6: Find all other reads of `state.note` as a String** — grep for `\.note` in `app.rs` and `ui/mod.rs`. Any place that treats it as a `String` now needs `.value`:

- In `entry_edit_validate()` (around line 905): `state.start_time_input` comparisons, but also save/submit code that reads `state.note` for the API call — change to `state.note.value`.
- In `build_edit_row()` (`ui/mod.rs` line ~920): the `edit_state.note` used in the Note field span — change to `edit_state.note.value` (rendering cursor handled in Step 7).

**Step 7: Update `build_edit_row()` Note field rendering** (`ui/mod.rs` around line 920):

When the Note field is focused, render cursor inline:

```rust
// Note field
let note_style = match edit_state.focused_field {
    EntryEditField::Note => Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD),
    _ => Style::default().fg(Color::White),
};
let note_value = if matches!(edit_state.focused_field, EntryEditField::Note) {
    let (before, after) = edit_state.note.split_at_cursor();
    let display = if edit_state.note.value.is_empty() {
        "[█]".to_string()
    } else {
        format!("[{}█{}]", before, after)
    };
    display
} else {
    format!(
        "[{}]",
        if edit_state.note.value.is_empty() { "None" } else { &edit_state.note.value }
    )
};
spans.push(Span::styled(note_value, note_style));
```

**Step 8: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```
Expected: compiles. Fix any remaining `.note` String references (add `.value`).

---

## Task 3: Migrate `description_input` to `TextInput`

**Files:**
- Modify: `toki-tui/src/app.rs`
- Modify: `toki-tui/src/ui/mod.rs`
- Modify: `toki-tui/src/main.rs`

**Step 1: Change field declaration** (line ~189):

```rust
// Before:
pub description_input: String,
// After:
pub description_input: TextInput,
```

**Step 2: Update `App::new()` initialiser** (line ~241):

```rust
// Before:
description_input: String::new(),
// After:
description_input: TextInput::new(),
```

**Step 3: Update `input_char()`** (line ~1299):

```rust
// Before:
self.description_input.push(c);
// After:
self.description_input.insert(c);
```

**Step 4: Update `input_backspace()`** (line ~1306):

```rust
// Before:
self.description_input.pop();
// After:
self.description_input.backspace();
```

**Step 5: Add `input_move_cursor()` and `input_cursor_home_end()` methods:**

```rust
pub fn input_move_cursor(&mut self, left: bool) {
    if self.editing_description {
        if left { self.description_input.move_left(); }
        else    { self.description_input.move_right(); }
    }
}

pub fn input_cursor_home_end(&mut self, home: bool) {
    if self.editing_description {
        if home { self.description_input.home(); }
        else    { self.description_input.end(); }
    }
}
```

**Step 6: Find all reads of `description_input` as a String:**

- `saved_timer_note` save/restore logic (around line 192–195 area, and navigate_to) — add `.value.clone()` / `TextInput::from_str(...)`.
- Any place that sets `description_input` to a String (e.g. `navigate_to` resetting it, loading from entry) — convert to `TextInput::from_str(...)` or `.value =`.

Grep: `description_input` in `app.rs`.

**Step 7: Update `Ctrl+X` clear in `main.rs`** (line ~308):

```rust
// Before:
app.description_input.clear();
// After:
app.description_input.clear();  // TextInput::clear() — same call, works as-is
```

**Step 8: Add left/right/Home/End key handlers in `main.rs`** — in the `EditDescription` view's `cwd_input.is_none()` branch (after existing Backspace handler, around line 296):

```rust
KeyCode::Left  => app.input_move_cursor(true),
KeyCode::Right => app.input_move_cursor(false),
KeyCode::Home  => app.input_cursor_home_end(true),
KeyCode::End   => app.input_cursor_home_end(false),
```

**Step 9: Update rendering in `ui/mod.rs`** (line ~1061):

```rust
// Before:
let input_text = format!("{}█", app.description_input);
// After:
let (before, after) = app.description_input.split_at_cursor();
let input_text = format!("{}█{}", before, after);
```

**Step 10: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```

---

## Task 4: Migrate `project_search_input` and `activity_search_input` to `TextInput`

**Files:**
- Modify: `toki-tui/src/app.rs`
- Modify: `toki-tui/src/ui/mod.rs`
- Modify: `toki-tui/src/main.rs`

**Step 1: Change field declarations** (lines ~173, ~178):

```rust
// Before:
pub project_search_input: String,
pub activity_search_input: String,
// After:
pub project_search_input: TextInput,
pub activity_search_input: TextInput,
```

**Step 2: Update `App::new()` initialisers** (lines ~233, ~236):

```rust
project_search_input: TextInput::new(),
activity_search_input: TextInput::new(),
```

**Step 3: Update `filter_projects()`** — the fuzzy matcher call reads `self.project_search_input` as a string (line ~1337):

```rust
// Before:
if self.project_search_input.is_empty() { ... }
matcher.fuzzy_match(&project.name, &self.project_search_input)
// After:
if self.project_search_input.value.is_empty() { ... }
matcher.fuzzy_match(&project.name, &self.project_search_input.value)
```

**Step 4: Update `filter_activities()`** — identical pattern for `activity_search_input.value`.

**Step 5: Update `search_input_char()` and `search_input_backspace()`:**

```rust
pub fn search_input_char(&mut self, c: char) {
    self.project_search_input.insert(c);
    self.filter_projects();
}
pub fn search_input_backspace(&mut self) {
    self.project_search_input.backspace();
    self.filter_projects();
}
pub fn search_input_clear(&mut self) {
    self.project_search_input.clear();
    self.filter_projects();
}
```

And the same for the activity equivalents (`activity_search_input_char`, etc.).

**Step 6: Add `search_move_cursor()` and `activity_search_move_cursor()` methods:**

```rust
pub fn search_move_cursor(&mut self, left: bool) {
    if left { self.project_search_input.move_left(); }
    else    { self.project_search_input.move_right(); }
}
pub fn search_cursor_home_end(&mut self, home: bool) {
    if home { self.project_search_input.home(); }
    else    { self.project_search_input.end(); }
}

pub fn activity_search_move_cursor(&mut self, left: bool) {
    if left { self.activity_search_input.move_left(); }
    else    { self.activity_search_input.move_right(); }
}
pub fn activity_search_cursor_home_end(&mut self, home: bool) {
    if home { self.activity_search_input.home(); }
    else    { self.activity_search_input.end(); }
}
```

**Step 7: Add key handlers in `main.rs`** — in `SelectProject` view, when `!app.selection_list_focused` (around line 130):

```rust
KeyCode::Left  => { if !app.selection_list_focused { app.search_move_cursor(true); } }
KeyCode::Right => { if !app.selection_list_focused { app.search_move_cursor(false); } }
KeyCode::Home  => { if !app.selection_list_focused { app.search_cursor_home_end(true); } }
KeyCode::End   => { if !app.selection_list_focused { app.search_cursor_home_end(false); } }
```

Same for `SelectActivity` view using activity equivalents.

**Step 8: Update rendering in `ui/mod.rs`** — project search (line ~243):

```rust
// Before:
let search_text = if app.project_search_input.is_empty() { ... }
    else if app.selection_list_focused { app.project_search_input.clone() }
    else { format!("{}█", app.project_search_input) };
// After:
let search_text = if app.project_search_input.value.is_empty() {
    if app.selection_list_focused { "Type to search...".to_string() }
    else { "█".to_string() }
} else if app.selection_list_focused {
    app.project_search_input.value.clone()
} else {
    let (before, after) = app.project_search_input.split_at_cursor();
    format!("{}█{}", before, after)
};
```

Same pattern for activity search.

**Step 9: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```

---

## Task 5: Migrate `cwd_input` to `TextInput`

**Files:**
- Modify: `toki-tui/src/app.rs`
- Modify: `toki-tui/src/ui/mod.rs`
- Modify: `toki-tui/src/main.rs`

**Step 1: Change field declaration** (line ~206):

```rust
// Before:
pub cwd_input: Option<String>,
// After:
pub cwd_input: Option<TextInput>,
```

**Step 2: Update `begin_cwd_change()`** (line ~1481):

```rust
// Before:
self.cwd_input = Some(self.git_context.cwd.to_string_lossy().to_string());
// After:
self.cwd_input = Some(TextInput::from_str(&self.git_context.cwd.to_string_lossy()));
```

**Step 3: Update `cwd_input_char()`:**

```rust
// Before:
if let Some(s) = &mut self.cwd_input { s.push(c); ... }
// After:
if let Some(s) = &mut self.cwd_input { s.insert(c); ... }
```

**Step 4: Update `cwd_input_backspace()`:**

```rust
// Before:
if let Some(s) = &mut self.cwd_input { s.pop(); ... }
// After:
if let Some(s) = &mut self.cwd_input { s.backspace(); ... }
```

**Step 5: Update `cwd_tab_complete()`** — it reads and writes `cwd_input` as a String. Change all reads to `.value` and all writes to assign `.value =` or use `TextInput::from_str`. Also call `.end()` after updating value so cursor moves to end of the new value.

**Step 6: Update `confirm_cwd_change()`** — reads `cwd_input` as a path string:

```rust
// Before:
if let Some(path_str) = &self.cwd_input { ... std::path::Path::new(path_str) ... }
// After:
if let Some(ti) = &self.cwd_input { ... std::path::Path::new(&ti.value) ... }
```

**Step 7: Add `cwd_move_cursor()` / `cwd_cursor_home_end()` methods:**

```rust
pub fn cwd_move_cursor(&mut self, left: bool) {
    if let Some(s) = &mut self.cwd_input {
        if left { s.move_left(); } else { s.move_right(); }
    }
}
pub fn cwd_cursor_home_end(&mut self, home: bool) {
    if let Some(s) = &mut self.cwd_input {
        if home { s.home(); } else { s.end(); }
    }
}
```

**Step 8: Add key handlers in `main.rs`** — in the `cwd_input.is_some()` branch of `EditDescription` (around line 252):

```rust
KeyCode::Left  => app.cwd_move_cursor(true),
KeyCode::Right => app.cwd_move_cursor(false),
KeyCode::Home  => app.cwd_cursor_home_end(true),
KeyCode::End   => app.cwd_cursor_home_end(false),
```

**Step 9: Update rendering in `ui/mod.rs`** (line ~1049):

```rust
// Before:
let input_text = format!("{}█{}", cwd_input, completions_hint);
// After:
let (before, after) = cwd_input.split_at_cursor();
let input_text = format!("{}█{}{}", before, after, completions_hint);
```

Note: `cwd_input` here is the `&TextInput` from the `if let Some(cwd_input) = &app.cwd_input` binding.

**Step 10: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```

---

## Task 6: Time input block cursor rendering

**Files:**
- Modify: `toki-tui/src/ui/mod.rs`

No state changes. The cursor position is `len()` of the time string. The colon at position 2 is synthetic (not typed), so the visible "digit slot" count is 4 (`HH`, `MM`). When `len == 5` the field is complete — no cursor. When `len < 5` — show `█` at position `len`, pad remaining slots with spaces.

**Step 1: Replace `build_edit_row()` start-time value computation** (lines ~853–857):

```rust
// Before:
let start_value = if edit_state.start_time_input.len() < 5 {
    format!("[{:>5}]", edit_state.start_time_input)
} else {
    format!("[{}]", edit_state.start_time_input)
};

// After:
let start_value = time_input_display(&edit_state.start_time_input);
```

**Step 2: Replace end-time value computation** (lines ~871–874) similarly:

```rust
let end_value = time_input_display(&edit_state.end_time_input);
```

**Step 3: Add helper function** (anywhere before `build_edit_row` in `ui/mod.rs`):

```rust
/// Render a partial or complete time string with a block cursor.
/// - len == 5 ("HH:MM"): display as-is, no cursor
/// - len < 5: show typed chars + '█' + space padding to fill 5-char slot
///   (the ':' at position 2 is counted as part of the 5 chars)
/// - len == 0: just '█' followed by 4 spaces
fn time_input_display(s: &str) -> String {
    if s.len() >= 5 {
        format!("[{}]", s)
    } else {
        // How many of the 5 visual slots are filled?
        let filled = s.len(); // 0..4
        let spaces = 5 - filled - 1; // slots left after cursor
        format!("[{}█{}]", s, " ".repeat(spaces))
    }
}
```

**Step 4: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```

Expected: compiles cleanly.

---

## Task 7: Wire left/right/Home/End for edit-mode note field in `main.rs`

**Files:**
- Modify: `toki-tui/src/main.rs`

The `entry_edit_move_cursor` and `entry_edit_cursor_home_end` methods were added in Task 2. Now wire them to key events.

**Step 1: Find the History view edit-mode key handler block** (around line 386). Add after the `Backspace` handler:

```rust
KeyCode::Left  => app.entry_edit_move_cursor(true),
KeyCode::Right => app.entry_edit_move_cursor(false),
KeyCode::Home  => app.entry_edit_cursor_home_end(true),
KeyCode::End   => app.entry_edit_cursor_home_end(false),
```

**Step 2: Find the Timer view (This Week) edit-mode key handler block** (around line 582). Add after the `Backspace` handler:

```rust
KeyCode::Left  => app.entry_edit_move_cursor(true),
KeyCode::Right => app.entry_edit_move_cursor(false),
KeyCode::Home  => app.entry_edit_cursor_home_end(true),
KeyCode::End   => app.entry_edit_cursor_home_end(false),
```

**Step 3: Run `cargo check`**

```
SQLX_OFFLINE=true cargo check
```

Expected: compiles cleanly. Only pre-existing dead code warnings.

---

## Task 8: Final verification

**Step 1: Run full check**

```
SQLX_OFFLINE=true cargo check
```

Expected: zero errors.

**Step 2: Manual smoke test** (run `cargo run` and verify):

- Time field: type `0` → `[0█   ]`, type `9` → `[09:█ ]`, type `3` → `[09:3█]`, type `0` → `[09:30]`
- Backspace from `09:30` → `[09:3█]`, backspace → `[09:█ ]`, backspace → `[0█   ]`, Enter/Ctrl+X → `[█    ]`
- Note editor: type text, use left/right to move cursor, Home/End jump to ends, backspace deletes char before cursor
- Project/activity search: same left/right/Home/End behaviour
- CWD input: same cursor navigation

