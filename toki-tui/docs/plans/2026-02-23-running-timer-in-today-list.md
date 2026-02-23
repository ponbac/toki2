# Running Timer in Today List — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show the running timer as the top row in the "This Week" today list, and allow editing its start time, project, activity, and note inline (End Time is not editable while running).

**Architecture:** Inject a synthetic row at index 0 of the this-week list when the timer is running. Reuse `EntryEditState` with sentinel `entry_id = -1` for the running timer's edit state. On Esc-confirm, write changes back to `App` fields (`absolute_start`, `selected_project`, `selected_activity`, `description_input`) — no DB write until the timer is saved normally. Add a dedicated save path `handle_running_timer_edit_save` that skips the DB entirely.

**Tech Stack:** Rust, ratatui, `time` crate, existing `EntryEditState` / `EntryEditField` types

---

## Key Constants / Sentinel Values

- `entry_id = -1` in `EntryEditState` means "editing the live running timer"
- The running timer row is always at `visible_entry_idx = 0` when `timer_state == Running`
- Existing DB-backed entries shift to indices 1..N when timer is running

---

## Task 1: Running-timer edit mode entry/exit in `app.rs`

**Files:**
- Modify: `toki-tui/src/app.rs`

**Context:**
- `enter_this_week_edit_mode` is at `app.rs:673`. It currently looks up the entry by index from `this_week_history()`.
- `create_edit_state` is at `app.rs:759`. It takes an `entry_id` and times.
- `entry_edit_next_field` / `entry_edit_prev_field` are at `app.rs:823` / `app.rs:848`. They hard-code the full `StartTime → EndTime → Project → Activity → Note` cycle.
- `exit_this_week_edit_mode` is at `app.rs:804`.

**Step 1: Update `enter_this_week_edit_mode` to handle running timer (index 0)**

Replace the body of `enter_this_week_edit_mode` (`app.rs:673–712`) so that when `timer_state == Running` and `focused_this_week_index == Some(0)`, it creates an edit state from live App fields instead of from `this_week_history()`.

The existing entries are now at shifted indices: DB entry at position `i` in `this_week_history()` is at `visible_entry_idx = i + 1`.

New logic:

```rust
pub fn enter_this_week_edit_mode(&mut self) {
    if let Some(idx) = self.focused_this_week_index {
        if self.timer_state == TimerState::Running && idx == 0 {
            // Editing the live running timer — sentinel entry_id = -1
            let start_time = self.absolute_start.unwrap_or_else(OffsetDateTime::now_utc);
            self.create_edit_state(
                -1,
                start_time,
                None, // no end time
                self.selected_project.as_ref().map(|p| p.id.clone()),
                self.selected_project.as_ref().map(|p| p.name.clone()),
                self.selected_activity.as_ref().map(|a| a.id.clone()),
                self.selected_activity.as_ref().map(|a| a.name.clone()),
                Some(self.description_input.value.clone()),
            );
        } else {
            // Editing a DB-backed entry — adjust index if timer is running
            let db_idx = if self.timer_state == TimerState::Running {
                idx.saturating_sub(1)
            } else {
                idx
            };
            let entry_data = self.this_week_history().get(db_idx).map(|e| {
                (
                    e.id,
                    e.start_time,
                    e.end_time,
                    e.project_id.clone(),
                    e.project_name.clone(),
                    e.activity_id.clone(),
                    e.activity_name.clone(),
                    e.note.clone(),
                )
            });

            if let Some((id, start_time, end_time, project_id, project_name,
                         activity_id, activity_name, note)) = entry_data
            {
                self.create_edit_state(
                    id, start_time, end_time,
                    project_id, project_name,
                    activity_id, activity_name, note,
                );
            }
        }
    }
}
```

**Step 2: Update `entry_edit_next_field` and `entry_edit_prev_field` to skip EndTime when editing running timer**

In both functions, when `entry_id == -1`, use a shortened cycle that skips `EndTime`:

In `entry_edit_next_field` (`app.rs:826–833`), change the `this_week_edit_state` arm:
```rust
if let Some(state) = &mut self.this_week_edit_state {
    state.focused_field = if state.entry_id == -1 {
        // Running timer: skip EndTime
        match state.focused_field {
            EntryEditField::StartTime => EntryEditField::Project,
            EntryEditField::Project => EntryEditField::Activity,
            EntryEditField::Activity => EntryEditField::Note,
            EntryEditField::Note => EntryEditField::StartTime,
            EntryEditField::EndTime => EntryEditField::Project, // shouldn't happen
        }
    } else {
        match state.focused_field {
            EntryEditField::StartTime => EntryEditField::EndTime,
            EntryEditField::EndTime => EntryEditField::Project,
            EntryEditField::Project => EntryEditField::Activity,
            EntryEditField::Activity => EntryEditField::Note,
            EntryEditField::Note => EntryEditField::StartTime,
        }
    };
    state.validation_error = None;
}
```

Apply the same pattern in `entry_edit_prev_field` (`app.rs:850–858`):
```rust
if let Some(state) = &mut self.this_week_edit_state {
    state.focused_field = if state.entry_id == -1 {
        match state.focused_field {
            EntryEditField::StartTime => EntryEditField::Note,
            EntryEditField::Project => EntryEditField::StartTime,
            EntryEditField::Activity => EntryEditField::Project,
            EntryEditField::Note => EntryEditField::Activity,
            EntryEditField::EndTime => EntryEditField::StartTime, // shouldn't happen
        }
    } else {
        match state.focused_field {
            EntryEditField::StartTime => EntryEditField::Note,
            EntryEditField::EndTime => EntryEditField::StartTime,
            EntryEditField::Project => EntryEditField::EndTime,
            EntryEditField::Activity => EntryEditField::Project,
            EntryEditField::Note => EntryEditField::Activity,
        }
    };
    state.validation_error = None;
}
```

**Step 3: Verify**

```bash
SQLX_OFFLINE=true cargo check
```
Expected: no errors.

---

## Task 2: `handle_running_timer_edit_save` in `main.rs`

**Files:**
- Modify: `toki-tui/src/main.rs`

**Context:**
- `handle_this_week_edit_save` is at `main.rs:938`. It parses times, looks up the entry date by `entry_id`, does a DB write, then refreshes history.
- We need a separate save path for `entry_id == -1` that writes back to `App` fields, validates the start time, and skips the DB.
- `absolute_start` is `Option<OffsetDateTime>` on `App`. Writing back requires converting the `HH:MM` input + today's local date into a UTC `OffsetDateTime`.
- Reject start time if it would be after `now`.

**Step 1: Add `handle_running_timer_edit_save` function after `handle_this_week_edit_save`**

Insert this new function at `main.rs` just after the closing `}` of `handle_this_week_edit_save` (around line 1011):

```rust
/// Apply edits from This Week edit mode back to the live running timer (no DB write).
/// Called when entry_id == -1 (sentinel for the running timer).
fn handle_running_timer_edit_save(app: &mut App) {
    let Some(state) = app.this_week_edit_state.take() else {
        return;
    };

    // Parse start time input
    let start_parts: Vec<&str> = state.start_time_input.split(':').collect();
    if start_parts.len() != 2 {
        app.set_status("Error: Invalid time format".to_string());
        return;
    }
    let Ok(start_hours) = start_parts[0].parse::<u8>() else {
        app.set_status("Error: Invalid start hour".to_string());
        return;
    };
    let Ok(start_mins) = start_parts[1].parse::<u8>() else {
        app.set_status("Error: Invalid start minute".to_string());
        return;
    };

    // Build new absolute_start: today's local date + typed HH:MM, converted to UTC
    let local_offset = time::UtcOffset::current_local_offset()
        .unwrap_or(time::UtcOffset::UTC);
    let today = time::OffsetDateTime::now_utc().to_offset(local_offset).date();
    let Ok(new_time) = time::Time::from_hms(start_hours, start_mins, 0) else {
        app.set_status("Error: Invalid start time".to_string());
        return;
    };
    let new_start = time::OffsetDateTime::new_in_offset(today, new_time, local_offset);

    // Reject if new start is in the future
    if new_start > time::OffsetDateTime::now_utc() {
        app.set_status("Error: Start time cannot be in the future".to_string());
        // Restore edit state so the user can correct it
        app.this_week_edit_state = Some(state);
        return;
    }

    // Write back to App fields
    app.absolute_start = Some(new_start.to_offset(time::UtcOffset::UTC));
    app.selected_project = state.project_id.zip(state.project_name).map(|(id, name)| {
        crate::app::TestProject { id, name }
    });
    app.selected_activity = state.activity_id.zip(state.activity_name).map(|(id, name)| {
        crate::app::TestActivity { id, name, project_id: String::new() }
    });
    app.description_input = crate::text_input::TextInput::from_str(&state.note.value);

    app.set_status("Running timer updated".to_string());
}
```

**Step 2: Update `handle_this_week_edit_save` to dispatch to the new function when `entry_id == -1`**

At the top of `handle_this_week_edit_save` (`main.rs:939`), add an early return for the running-timer case:

```rust
async fn handle_this_week_edit_save(app: &mut App, db: &api::Database) -> Result<()> {
    // Running timer edits don't touch the DB
    if app.this_week_edit_state.as_ref().map(|s| s.entry_id) == Some(-1) {
        handle_running_timer_edit_save(app);
        return Ok(());
    }
    // ... rest of existing function unchanged
```

**Step 3: Verify**

```bash
SQLX_OFFLINE=true cargo check
```
Expected: no errors. Note: `TestProject` and `TestActivity` field names — double-check them in `app.rs` (look for `pub struct TestProject` and `pub struct TestActivity`). Adjust field names in `handle_running_timer_edit_save` if needed.

---

## Task 3: Render the running timer row in `ui/mod.rs`

**Files:**
- Modify: `toki-tui/src/ui/mod.rs`

**Context:**
- `render_this_week_history` is at `ui/mod.rs:631`.
- The synthetic running-timer row is always rendered first (before date separators and before existing entries), at `visible_entry_idx = 0`.
- Display format: `"HH:MM - ▶  [▶ 00h:05m] | Project - Activity | Note"` — use `▶` as the play symbol. End time slot shows `"▶   "` (arrow + spaces to match width of `"HH:MM"`).
- Duration shows elapsed time in `[▶ 00h:05m]` format using `app.elapsed_duration()`.
- When focused: same inverse-video highlight (black-on-white bold) as other rows.
- When in edit mode (`this_week_edit_state.entry_id == -1`): call a new `build_running_timer_edit_row` function (no End Time field).
- The block title should count the running timer: `" This Week (N entries) "` becomes `" This Week (N entries + running) "` when timer is running.

**Step 1: Add `build_running_timer_display_row` function after `build_display_row` (around `ui/mod.rs:851`)**

```rust
fn build_running_timer_display_row(app: &App, is_focused: bool) -> Line<'static> {
    let start_str = app.absolute_start
        .map(|t| {
            let local = to_local_time(t);
            format!("{:02}:{:02}", local.hour(), local.minute())
        })
        .unwrap_or_else(|| "??:??".to_string());

    let elapsed = app.elapsed_duration();
    let total_mins = elapsed.as_secs() / 60;
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    let duration_str = format!("[▶ {:02}h:{:02}m]", hours, mins);

    let project = app.selected_project.as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let activity = app.selected_activity.as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let note = app.description_input.value.clone();

    let max_note_len = 30;
    let note_display = if note.is_empty() {
        String::new()
    } else if note.len() > max_note_len {
        format!("{}[...]", &note[..max_note_len])
    } else {
        note.clone()
    };

    let text = if note_display.is_empty() {
        format!(
            "{} - ▶   {} | {} - {}",
            start_str, duration_str, project, activity
        )
    } else {
        format!(
            "{} - ▶   {} | {} - {} | {}",
            start_str, duration_str, project, activity, note_display
        )
    };

    if is_focused {
        return Line::from(Span::styled(
            text,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Non-focused: color each part
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(format!("{} - ", start_str), Style::default().fg(Color::Yellow)),
        Span::styled("▶   ", Style::default().fg(Color::Green)),
        Span::styled(duration_str, Style::default().fg(Color::Green)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} - {}", project, activity), Style::default().fg(Color::Cyan)),
    ];
    if !note_display.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(note_display, Style::default().fg(Color::Gray)));
    }
    Line::from(spans)
}
```

**Step 2: Add `build_running_timer_edit_row` function after `build_running_timer_display_row`**

This is like `build_edit_row` but without the End Time field:

```rust
fn build_running_timer_edit_row(edit_state: &EntryEditState) -> Line<'_> {
    let mut spans = vec![];

    // Start time field
    let start_value = time_input_display(&edit_state.start_time_input);
    let start_style = match edit_state.focused_field {
        EntryEditField::StartTime => Style::default()
            .fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(start_value, start_style));

    // Separator (no end time — show ▶ placeholder)
    spans.push(Span::styled(" - ▶   | ", Style::default().fg(Color::White)));

    // Project field
    let project_value = format!("[{}]", edit_state.project_name.as_deref().unwrap_or("None"));
    let project_style = match edit_state.focused_field {
        EntryEditField::Project => Style::default()
            .fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(project_value, project_style));

    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // Activity field
    let activity_value = format!("[{}]", edit_state.activity_name.as_deref().unwrap_or("None"));
    let activity_style = match edit_state.focused_field {
        EntryEditField::Activity => Style::default()
            .fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(activity_value, activity_style));

    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Note field
    let note_style = match edit_state.focused_field {
        EntryEditField::Note => Style::default()
            .fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    let note_value = if matches!(edit_state.focused_field, EntryEditField::Note) {
        let (before, after) = edit_state.note.split_at_cursor();
        if edit_state.note.value.is_empty() {
            "[█]".to_string()
        } else {
            format!("[{}█{}]", before, after)
        }
    } else {
        format!(
            "[{}]",
            if edit_state.note.value.is_empty() { "None" } else { &edit_state.note.value }
        )
    };
    spans.push(Span::styled(note_value, note_style));

    Line::from(spans)
}
```

**Step 3: Update `render_this_week_history` to inject the running-timer row**

In `render_this_week_history` (`ui/mod.rs:631`):

1. Update the block title to include `+ running` when timer is running:
```rust
let title = if app.timer_state == crate::app::TimerState::Running {
    format!(" This Week ({} entries + running) ", this_week_entries.len())
} else {
    format!(" This Week ({} entries) ", this_week_entries.len())
};
let block = Block::default()
    .borders(Borders::ALL)
    .title(title)
    // ... rest unchanged
```

2. Remove the early return on empty (`this_week_entries.is_empty()`) — the running timer row should show even with zero completed entries. Instead wrap it: only skip the separator/entry loop if empty.

3. Before the entry loop, inject the running-timer row when `timer_state == Running`:
```rust
// Inject running timer row at index 0
if app.timer_state == crate::app::TimerState::Running && row_count < max_rows {
    let is_focused = is_today_focused && app.focused_this_week_index == Some(0);
    let is_editing = app.this_week_edit_state.as_ref()
        .map(|s| s.entry_id == -1)
        .unwrap_or(false);

    let line = if is_editing {
        build_running_timer_edit_row(app.this_week_edit_state.as_ref().unwrap())
    } else {
        build_running_timer_display_row(app, is_focused)
    };

    let row_rect = Rect::new(inner_area.x, row_y, inner_area.width, 1);
    frame.render_widget(Paragraph::new(line).style(Style::default().fg(Color::White)), row_rect);
    row_y += 1;
    row_count += 1;
    visible_entry_idx += 1;  // completed entries now start at visible_entry_idx = 1
}
```

4. The existing entry loop remains unchanged — `visible_entry_idx` is already 1 for the first DB entry when timer is running.

**Step 4: Verify**

```bash
SQLX_OFFLINE=true cargo check
```
Expected: no errors.

---

## Task 4: Navigation — keep `focused_this_week_index` in bounds when timer starts/stops

**Files:**
- Modify: `toki-tui/src/app.rs`

**Context:**
- When the timer starts, the running-timer row is inserted at index 0, shifting all DB entries up by 1. If `focused_this_week_index` was pointing at a DB entry (e.g. `Some(0)`), it now points at the running-timer row instead.
- When the timer stops (i.e. is saved), the running-timer row disappears and all indices shift back down. If focus was at `Some(0)` (the running row), it should reset to `None` or `Some(0)` pointing at the first DB entry.
- `start_timer` is at `app.rs:393`. `update_history` is at `app.rs:434`.

**Step 1: In `start_timer`, shift `focused_this_week_index` up by 1 if set**

```rust
pub fn start_timer(&mut self) {
    self.timer_state = TimerState::Running;
    self.absolute_start = Some(OffsetDateTime::now_utc());
    self.local_start = Some(Instant::now());
    // Shift focus: running timer row is inserted at index 0
    if let Some(idx) = self.focused_this_week_index {
        self.focused_this_week_index = Some(idx + 1);
    }
}
```

**Step 2: In `update_history` (called after saving/stopping timer at `app.rs:434`), reset focus if it was on the running timer row**

`update_history` is called after the timer is saved. At that point `timer_state` may have already been set to `Stopped` (or about to be). The index shift needs to happen when `timer_state` transitions from Running to Stopped.

Find where `timer_state` is set to `Stopped` after a save. This happens in `handle_save_timer_with_action` in `main.rs` (look for `app.timer_state = TimerState::Stopped` or `app.stop_timer()`). After that line, add:

```rust
// Running timer row disappears — shift focus back down
if let Some(idx) = app.focused_this_week_index {
    app.focused_this_week_index = if idx == 0 {
        None // was on the running row, which no longer exists
    } else {
        Some(idx.saturating_sub(1))
    };
}
```

**Step 3: Verify**

```bash
SQLX_OFFLINE=true cargo check
```
Expected: no errors.

---

## Task 5: Final verification

**Step 1: Compile check**

```bash
SQLX_OFFLINE=true cargo check
```
Expected: no errors, only pre-existing warnings.

**Step 2: Run tests**

```bash
SQLX_OFFLINE=true cargo test
```
Expected: all 6 tests pass.

**Step 3: Manual smoke test checklist**

- Start timer → running-timer row appears at top of Today list, green ▶ symbol, live elapsed updates
- Navigate to Today list with Tab, use j/k to focus the running row (index 0)
- Press Enter to edit → `[HH:MM]` start time, `[Project]`, `[Activity]`, `[Note]` shown; no End Time field
- Tab cycles Start → Project → Activity → Note → Start (skips End Time)
- Edit start time with digits → auto-inserts `:`, rejects future times
- Press Enter on Project → navigates to project picker, returns, edit state preserved
- Press Enter on Activity → navigates to activity picker, returns, edit state preserved
- Press Enter on Note → navigates to description editor, returns, edit state preserved
- Esc from edit → changes written back to running timer display; no DB write
- Save/stop timer → running row disappears; saved entry appears in list at correct position; focus adjusts correctly
- With no completed entries today: running row still shows; "This Week (0 entries + running)" in title
