# TUI Save Action Dialog Design

**Date:** 2025-02-11  
**Status:** Approved  
**Context:** Add user choice when saving timer entries in toki-tui

## Overview

Add a modal dialog when user presses Ctrl+S to choose what happens after saving: continue with same project, continue with new project, or stop timer completely.

## Problem

Current behavior always saves and starts a new timer with the same project/activity. Users may want:
- To continue with a different project after saving
- To stop working after saving (no new timer)
- To explicitly confirm their choice each time

## UX Flow

### Save Dialog Trigger

When user presses **Ctrl+S**:
1. Validate project/activity is selected (existing validation)
2. Show modal dialog in center of screen
3. Wait for user selection

### Dialog Layout

```
┌─ Save Timer ─────────────────────────────┐
│                                           │
│  1. Save & continue (same project)        │
│  2. Save & continue (new project)         │
│  3. Save & stop                           │
│  4. Cancel                                │
│                                           │
└───────────────────────────────────────────┘
```

**Dimensions:** 
- Width: ~45 characters
- Height: 8 rows (including border)
- Position: Centered on screen

**Visual Style:**
- Background: Semi-transparent overlay (dim background)
- Selected option: Bold + Yellow text
- Other options: White text
- Border: Standard borders with title

### Input Handling

**Quick selection (number keys):**
- `1` → Save & continue (same project)
- `2` → Save & continue (new project)
- `3` → Save & stop
- `4` → Cancel

**Navigation:**
- `↑` / `k` → Move selection up
- `↓` / `j` → Move selection down
- `Enter` → Confirm selected action
- `Esc` / `q` → Cancel (same as option 4)

### Option Behaviors

**1. Save & continue (same project)**
- Save current timer to database
- Start new timer immediately
- Keep same project/activity
- Clear description (reset to empty/default)
- Show status: "Saved [duration] to [Project] / [Activity]"

**2. Save & continue (new project)**
- Save current timer to database
- Start new timer immediately
- Clear project/activity/description (all reset)
- Show status: "Saved [duration]. Timer started. Press P to select project."

**3. Save & stop**
- Save current timer to database
- Stop timer completely (no new timer)
- Keep project/activity/description for next session
- Show status: "Saved [duration] to [Project] / [Activity]"

**4. Cancel**
- Return to Timer view without saving
- Timer continues running
- No status message

## Technical Implementation

### State Changes (`app.rs`)

**Add new View:**
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Timer,
    History,
    SelectProject,
    SelectActivity,
    EditDescription,
    SaveAction,  // NEW: Save action dialog
}
```

**Add SaveAction enum:**
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SaveAction {
    ContinueSameProject,   // Option 1
    ContinueNewProject,    // Option 2
    SaveAndStop,           // Option 3
    Cancel,                // Option 4
}
```

**Add to App struct:**
```rust
pub struct App {
    // ... existing fields ...
    pub selected_save_action: SaveAction,  // Currently highlighted option
}
```

**Initialize in App::new():**
```rust
selected_save_action: SaveAction::ContinueSameProject,  // Default to option 1
```

**Add navigation methods:**
```rust
impl App {
    pub fn select_next_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::ContinueNewProject,
            SaveAction::ContinueNewProject => SaveAction::SaveAndStop,
            SaveAction::SaveAndStop => SaveAction::Cancel,
            SaveAction::Cancel => SaveAction::ContinueSameProject,
        };
    }
    
    pub fn select_previous_save_action(&mut self) {
        self.selected_save_action = match self.selected_save_action {
            SaveAction::ContinueSameProject => SaveAction::Cancel,
            SaveAction::ContinueNewProject => SaveAction::ContinueSameProject,
            SaveAction::SaveAndStop => SaveAction::ContinueNewProject,
            SaveAction::Cancel => SaveAction::SaveAndStop,
        };
    }
    
    pub fn select_save_action_by_number(&mut self, num: u32) {
        self.selected_save_action = match num {
            1 => SaveAction::ContinueSameProject,
            2 => SaveAction::ContinueNewProject,
            3 => SaveAction::SaveAndStop,
            4 => SaveAction::Cancel,
            _ => return,
        };
    }
}
```

### UI Rendering (`ui/mod.rs`)

**Add to render() function:**
```rust
pub fn render(frame: &mut Frame, app: &App) {
    match app.current_view {
        // ... existing views ...
        View::SaveAction => render_save_action_dialog(frame, app),
    }
}
```

**New render function:**
```rust
fn render_save_action_dialog(frame: &mut Frame, app: &App) {
    // Calculate centered position (45 cols x 8 rows)
    let area = centered_rect(45, 8, frame.size());
    
    // Clear background area
    frame.render_widget(Clear, area);
    
    // Render options as list items
    let options = vec![
        "1. Save & continue (same project)",
        "2. Save & continue (new project)",
        "3. Save & stop",
        "4. Cancel",
    ];
    
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let action = match i {
                0 => SaveAction::ContinueSameProject,
                1 => SaveAction::ContinueNewProject,
                2 => SaveAction::SaveAndStop,
                3 => SaveAction::Cancel,
                _ => unreachable!(),
            };
            
            let style = if action == app.selected_save_action {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            
            ListItem::new(*text).style(style)
        })
        .collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Save Timer ")
                .padding(Padding::horizontal(1))
        );
    
    frame.render_widget(list, area);
}

// Helper function for centering
fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((r.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((r.height.saturating_sub(height)) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((r.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Length((r.width.saturating_sub(width)) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

**Add Clear widget import:**
```rust
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph};
```

### Input Handling (`main.rs`)

**Change Ctrl+S behavior:**
```rust
// In Timer view
KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    // Validate first
    if app.timer_state == app::TimerState::Stopped {
        app.set_status("No active timer to save".to_string());
    } else if !app.has_project_activity() {
        app.set_status("Cannot save: Please select Project / Activity first (press P)".to_string());
    } else {
        // Show save action dialog
        app.navigate_to(app::View::SaveAction);
    }
}
```

**Add SaveAction view handler:**
```rust
app::View::SaveAction => {
    match key.code {
        KeyCode::Char('1') => {
            app.select_save_action_by_number(1);
            handle_save_timer_with_action(app, &db).await?;
        }
        KeyCode::Char('2') => {
            app.select_save_action_by_number(2);
            handle_save_timer_with_action(app, &db).await?;
        }
        KeyCode::Char('3') => {
            app.select_save_action_by_number(3);
            handle_save_timer_with_action(app, &db).await?;
        }
        KeyCode::Char('4') | KeyCode::Esc | KeyCode::Char('q') => {
            // Cancel - return to timer view
            app.navigate_to(app::View::Timer);
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_previous_save_action(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_save_action(),
        KeyCode::Enter => {
            handle_save_timer_with_action(app, &db).await?;
        }
        _ => {}
    }
}
```

**Update handle_save_timer function:**
```rust
async fn handle_save_timer_with_action(app: &mut App, db: &api::Database) -> Result<()> {
    // Handle Cancel first
    if app.selected_save_action == app::SaveAction::Cancel {
        app.navigate_to(app::View::Timer);
        return Ok(());
    }
    
    // Validate and save (existing logic)
    if let Some(start_time) = app.absolute_start {
        let end_time = time::OffsetDateTime::now_utc();
        let duration = app.elapsed_duration();
        
        let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
        let project_name = Some(app.current_project_name());
        let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
        let activity_name = Some(app.current_activity_name());
        let note = Some(app.description_input.clone());
        
        // Save to database
        match db.save_timer_entry(
            app.user_id,
            start_time,
            end_time,
            project_id,
            project_name.clone(),
            activity_id,
            activity_name.clone(),
            note,
        ).await {
            Ok(_) => {
                // Format status message
                let hours = duration.as_secs() / 3600;
                let minutes = (duration.as_secs() % 3600) / 60;
                let seconds = duration.as_secs() % 60;
                let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);
                
                let project_display = project_name.unwrap_or_else(|| "[None]".to_string());
                let activity_display = activity_name.unwrap_or_else(|| "[None]".to_string());
                
                // Refresh history
                if let Ok(history) = db.get_timer_history(app.user_id, 50).await {
                    app.update_history(history);
                }
                
                // Handle action-specific behavior
                match app.selected_save_action {
                    app::SaveAction::ContinueSameProject => {
                        // Keep project/activity, clear description
                        app.description_input.clear();
                        app.description_is_default = true;
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {} to {} / {}",
                            duration_str, project_display, activity_display
                        ));
                    }
                    app::SaveAction::ContinueNewProject => {
                        // Clear everything
                        app.selected_project = None;
                        app.selected_activity = None;
                        app.description_input.clear();
                        app.description_is_default = true;
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {}. Timer started. Press P to select project.",
                            duration_str
                        ));
                    }
                    app::SaveAction::SaveAndStop => {
                        // Stop timer, keep everything
                        app.timer_state = app::TimerState::Stopped;
                        app.absolute_start = None;
                        app.local_start = None;
                        app.set_status(format!(
                            "Saved {} to {} / {}",
                            duration_str, project_display, activity_display
                        ));
                    }
                    app::SaveAction::Cancel => unreachable!(), // Handled above
                }
                
                // Return to timer view
                app.navigate_to(app::View::Timer);
            }
            Err(e) => {
                app.set_status(format!("Error saving timer: {}", e));
                app.navigate_to(app::View::Timer);
            }
        }
    } else {
        app.set_status("Error: No start time recorded".to_string());
        app.navigate_to(app::View::Timer);
    }
    
    Ok(())
}
```

## Implementation Steps

1. Add `SaveAction` enum and `View::SaveAction` to `app.rs`
2. Add `selected_save_action` field to `App` struct
3. Add navigation methods to `App` impl block
4. Add `render_save_action_dialog()` to `ui/mod.rs`
5. Add `Clear` widget import to `ui/mod.rs`
6. Update Ctrl+S handler in `main.rs` to show dialog
7. Add `View::SaveAction` input handling in `main.rs`
8. Replace `handle_save_timer()` with `handle_save_timer_with_action()` in `main.rs`
9. Test all four save actions

## Success Criteria

- Pressing Ctrl+S shows centered modal dialog
- Number keys 1-4 immediately select and execute action
- Arrow keys / j/k navigate options with yellow highlight
- Enter confirms selected option
- Esc/q cancels without saving
- Option 1: Saves and continues with same project/activity (clears description)
- Option 2: Saves and continues with cleared project/activity/description
- Option 3: Saves and stops timer completely
- Option 4: Returns to timer view without saving
- Status messages accurately reflect the action taken
