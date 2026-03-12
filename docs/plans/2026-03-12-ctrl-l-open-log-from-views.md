# Design: Ctrl+L to Open Log from Timer and History Views

**Date:** 2026-03-12

## Summary

Allow opening a linked log file directly from Timer view (Today panel) and History view by pressing `Ctrl+L` on a selected entry. This is a read-only-entry / editable-log operation — the entry note cannot be modified from these views, but the log file itself is fully editable in `$EDITOR`.

## Behaviour

| Condition | Result |
|-----------|--------|
| Entry selected, note has `[log:XXXXXX]` | Open log file in `$EDITOR` |
| Entry selected, no log tag in note | Status: `"No log linked to this entry"` |
| Log file referenced but missing on disk | Status: `"Log file not found"` |
| No entry selected | Key is ignored |
| Locked entry | Allowed — log is always editable |

## New Action

`Action::OpenEntryLogNote(String)` in `action_queue.rs` carries the resolved log ID. Distinct from `Action::OpenLogNote` (create-or-open for running note).

## Handler

New `async fn handle_open_entry_log_note(id: &str, app: &mut App) -> anyhow::Result<()>` in `actions.rs`:

1. `log_notes::log_path(id)?`
2. If file doesn't exist → `app.set_status("Log file not found"); return Ok(())`
3. `crate::editor::open_editor(&path).await?`
4. `app.needs_full_redraw = true`

No mutation of `description_log_id` or running-timer state.

## Key Bindings

**Timer view** (`views/timer.rs`): `Ctrl+L` when `is_persisted_today_row_selected(app)` and `!is_editing_this_week(app)`. Resolves the entry using the same index-offset logic as `Ctrl+R`.

**History view** (`views/history.rs`): `Ctrl+L` in the non-edit-mode branch when `focused_history_index.is_some()`.

No conflict: in history edit-mode, `Char('l')` is used for field navigation but `Ctrl+L` is free.

## UI Hints

Add `Ctrl+L: Log` to hint bars in `timer_view.rs` and `history_view.rs`, near `Ctrl+R`.

## Out of Scope

- Creating a new log from these views (Notes view only).
- Modifying the entry note/fields (locked/normal edit paths unchanged).
