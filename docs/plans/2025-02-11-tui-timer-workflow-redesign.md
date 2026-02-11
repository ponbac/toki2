# TUI Timer Workflow Redesign

**Date:** 2025-02-11  
**Status:** Approved

## Problem

Current TUI creates timer entries in the database immediately when starting/changing projects. This creates unwanted history entries. Users want to:
- Start a timer and let it run
- Change project/activity multiple times while running
- Edit description while running
- Save only when ready (explicit action)
- Continue with a new timer after saving

## Solution Overview

Move to a local-state-first model where the active timer lives only in app memory until explicitly saved with Ctrl+S.

## Timer States

### Simplified State Model

```rust
pub enum TimerState {
    Stopped,    // No active timer
    Running,    // Timer counting
}
```

No pause state - timers run continuously once started.

### State Transitions

**Start timer (Space or Ctrl+K):**
- `Stopped` → `Running`
- `Running` → No-op (show "Timer already running")

**Save timer (Ctrl+S):**
- `Running` → Save to DB, auto-start new timer (stays `Running`)
- `Stopped` → No-op (show "No active timer to save")

**Project/Activity/Description changes:**
- Update local state only
- No database operations
- Works in any state

## Duration Tracking

### Fields in App

```rust
pub absolute_start: Option<OffsetDateTime>,  // UTC time when timer started
pub local_start: Option<Instant>,            // For UI duration display
pub timer_state: TimerState,
// Remove: paused_duration, pause_start
```

### Duration Calculation

```rust
fn elapsed_duration(&self) -> Duration {
    match self.timer_state {
        TimerState::Stopped => Duration::ZERO,
        TimerState::Running => {
            self.local_start
                .map(|start| start.elapsed())
                .unwrap_or_default()
        }
    }
}
```

## Database Changes

### Remove Methods

- `get_active_timer()` - No longer track active timer in DB
- `start_timer()` - Was creating entries prematurely
- `stop_timer()` - Was deleting unsaved entries
- `update_timer_note()` - No live updates needed

### Add Method

```rust
pub async fn save_timer_entry(
    &self,
    user_id: i32,
    start_time: OffsetDateTime,
    end_time: OffsetDateTime,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
    note: Option<String>,
) -> Result<TimerHistoryEntry>
```

Inserts a completed entry with `end_time` set.

### Keep Methods

- `get_timer_history()` - Still needed for history view

## Key Bindings

### Timer Control

- **Space** - Start timer (backward compat)
- **Ctrl+K** - Start timer (matches web app)
- **Ctrl+S** - Save & continue (save current, auto-start new)

### Navigation (Unchanged)

- **j/k** - Navigate between boxes
- **Enter** - Activate focused box (disabled for Timer box)
- **H** - History view
- **Q / Ctrl+C** - Quit

## Save Behavior

### When Ctrl+S Pressed (Running State)

1. **Save to database:**
   - `start_time`: `absolute_start` value
   - `end_time`: `OffsetDateTime::now_utc()`
   - `project_id/name`: Current selection or None
   - `activity_id/name`: Current selection or None
   - `note`: Current `description_input`

2. **Reset for new timer:**
   - `absolute_start = OffsetDateTime::now_utc()`
   - `local_start = Instant::now()`
   - `timer_state = Running` (continues)
   - Keep `selected_project` and `selected_activity`
   - Reset `description_input = "Doing something important..."`
   - Reset `description_is_default = true`

3. **Update UI:**
   - Status: `"Saved HH:MM:SS to [Project] / [Activity]. New timer started."`
   - Refresh "Today" history section

### Validation Messages

- Ctrl+S when `Stopped`: `"No active timer to save"`
- Space/Ctrl+K when `Running`: `"Timer already running (Ctrl+S to save)"`

## UI Changes

### Timer Box Display

**Stopped:**
```
┌─ Timer [FOCUSED] ───┐
│ 00:00:00            │
│ [Space/Ctrl+K]      │
└─────────────────────┘
```

**Running:**
```
┌─ Timer ─────────────┐
│ 01:23:45 ⏵          │
│ [Ctrl+S: Save]      │
└─────────────────────┘
```

### Controls Footer

```
Space/Ctrl+K: Start  Ctrl+S: Save  j/k: Navigate  Enter: Activate  H: History  Q: Quit
```

### Enter Key Behavior

When Timer box is focused: Do nothing (disabled)

## App Initialization

On startup:
- Start in `Stopped` state
- Don't load "active timer" from database
- User manually starts first timer

Future: Add persistence for unsaved timer state (separate table).

## Implementation Steps

1. Update `App` struct - remove pause fields, keep `absolute_start`
2. Update `TimerState` enum - remove `Paused` variant
3. Remove database methods for active timer management
4. Add `save_timer_entry()` method
5. Update key handlers - Space/Ctrl+K for start, Ctrl+S for save
6. Remove `update_running_timer_project()` function (no longer needed)
7. Update `handle_toggle_timer()` to just start (rename to `handle_start_timer()`)
8. Add `handle_save_timer()` function
9. Update UI rendering - new timer box states, controls footer
10. Remove app initialization of active timer from DB
11. Test full workflow

## Testing Checklist

- [ ] Start timer with Space - verify running
- [ ] Start timer with Ctrl+K - verify running
- [ ] Press Space/Ctrl+K when running - verify no-op message
- [ ] Change project while running - verify updates immediately
- [ ] Change activity while running - verify updates immediately
- [ ] Edit description while running - verify updates
- [ ] Save with Ctrl+S - verify entry in history, new timer started
- [ ] Verify saved entry has correct start/end times
- [ ] Verify new timer keeps project/activity, resets description
- [ ] Press Ctrl+S when stopped - verify no-op message
- [ ] Check "Today" section updates after save
