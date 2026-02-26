use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::action_queue::{Action, ActionTx};
use super::super::actions::handle_entry_edit_enter;
use super::enqueue_action;

pub(super) fn handle_timer_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match key.code {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        // Ctrl+C also quits
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        // Ctrl+S: Save & continue
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.timer_state == app::TimerState::Stopped {
                app.set_status("No active timer to save".to_string());
            } else if !app.has_project_activity() {
                app.set_status(
                    "Cannot save: Please select Project / Activity first (press P)".to_string(),
                );
            } else {
                app.navigate_to(app::View::SaveAction);
            }
        }
        KeyCode::Tab => {
            if is_editing_this_week(app) {
                app.entry_edit_next_field();
            } else {
                app.focus_next();
            }
        }
        KeyCode::BackTab => {
            if is_editing_this_week(app) {
                app.entry_edit_prev_field();
            } else {
                app.focus_previous();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if is_editing_this_week(app) {
                app.entry_edit_next_field();
            } else if app.focused_box == app::FocusedBox::Today {
                app.this_week_focus_down();
            } else {
                app.focus_next();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if is_editing_this_week(app) {
                app.entry_edit_prev_field();
            } else if app.focused_box == app::FocusedBox::Today {
                app.this_week_focus_up();
            } else {
                app.focus_previous();
            }
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
            if is_editing_this_week(app) {
                app.entry_edit_next_field();
            }
        }
        KeyCode::Left => {
            if is_editing_this_week(app) {
                app.entry_edit_prev_field();
            }
        }
        KeyCode::Char('h') | KeyCode::Char('H') => {
            if is_editing_this_week(app) {
                app.entry_edit_prev_field();
            } else {
                enqueue_action(action_tx, Action::LoadHistoryAndOpen);
            }
        }
        KeyCode::Home => {
            if is_editing_this_week(app) {
                app.entry_edit_cursor_home_end(true);
            }
        }
        KeyCode::End => {
            if is_editing_this_week(app) {
                app.entry_edit_cursor_home_end(false);
            }
        }
        KeyCode::Enter => {
            handle_enter_key(app, action_tx);
        }
        // Number keys for time input in edit mode
        KeyCode::Char(c) if is_editing_this_week(app) && c.is_ascii_digit() => {
            app.entry_edit_input_char(c);
        }
        KeyCode::Backspace => {
            if is_editing_this_week(app) {
                if !is_note_focused_in_this_week_edit(app) {
                    app.entry_edit_backspace();
                }
            } else if is_persisted_today_row_selected(app) {
                app.enter_delete_confirm(app::DeleteOrigin::Timer);
            }
        }
        KeyCode::Esc => {
            handle_escape_key(app, action_tx);
        }
        KeyCode::Char(' ') => {
            handle_space_key(app, action_tx);
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            app.navigate_to(app::View::SelectProject);
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.navigate_to(app::View::EditDescription);
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.toggle_timer_size();
        }
        // S: Open Statistics view (unmodified only - Ctrl+S is save)
        KeyCode::Char('s') | KeyCode::Char('S')
            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.navigate_to(app::View::Statistics);
        }
        KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            handle_ctrl_x_key(app, action_tx);
        }
        KeyCode::Delete if !is_editing_this_week(app) && is_persisted_today_row_selected(app) => {
            app.enter_delete_confirm(app::DeleteOrigin::Timer);
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => app.toggle_zen_mode(),
        _ => {}
    }
}

fn is_editing_this_week(app: &App) -> bool {
    app.this_week_edit_state.is_some()
}

fn is_note_focused_in_this_week_edit(app: &App) -> bool {
    app.this_week_edit_state
        .as_ref()
        .is_some_and(|s| s.focused_field == app::EntryEditField::Note)
}

fn is_persisted_today_row_selected(app: &App) -> bool {
    app.focused_box == app::FocusedBox::Today
        && app
            .focused_this_week_index
            .is_some_and(|idx| !(app.timer_state == app::TimerState::Running && idx == 0))
}

fn handle_enter_key(app: &mut App, action_tx: &ActionTx) {
    if is_editing_this_week(app) {
        // In edit mode, Enter on Start/End advances field; other fields open modal.
        if let Some(state) = &app.this_week_edit_state {
            match state.focused_field {
                app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                    app.entry_edit_next_field();
                }
                _ => {
                    handle_entry_edit_enter(app);
                }
            }
        }
        return;
    }

    match app.focused_box {
        app::FocusedBox::Timer => {
            enqueue_action(action_tx, Action::StartTimer);
        }
        app::FocusedBox::Today => {
            if app.focused_this_week_index.is_none() && !app.this_week_history().is_empty() {
                app.focused_this_week_index = Some(0);
            }
            app.enter_this_week_edit_mode();
        }
        _ => {
            app.activate_focused_box();
        }
    }
}

fn handle_escape_key(app: &mut App, action_tx: &ActionTx) {
    if app.zen_mode {
        app.exit_zen_mode();
        return;
    }

    if is_editing_this_week(app) {
        if let Some(error) = app.entry_edit_validate() {
            app.entry_edit_revert_invalid_times();
            app.set_status(format!("Edit cancelled: {}", error));
            app.exit_this_week_edit_mode();
            app.focused_box = app::FocusedBox::Today;
        } else {
            enqueue_action(action_tx, Action::SaveThisWeekEdit);
        }
        return;
    }

    app.focused_box = app::FocusedBox::Timer;
    app.focused_this_week_index = None;
}

fn handle_space_key(app: &mut App, action_tx: &ActionTx) {
    match app.timer_state {
        app::TimerState::Stopped => {
            enqueue_action(action_tx, Action::StartTimer);
        }
        app::TimerState::Running => {
            if !app.has_project_activity() {
                app.set_status(
                    "Cannot save: Please select Project / Activity first (press P)".to_string(),
                );
            } else {
                app.selected_save_action = app::SaveAction::SaveAndStop;
                enqueue_action(action_tx, Action::SaveTimer);
            }
        }
    }
}

fn handle_ctrl_x_key(app: &mut App, action_tx: &ActionTx) {
    if is_editing_this_week(app) {
        if let Some(state) = &app.this_week_edit_state {
            match state.focused_field {
                app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                    app.entry_edit_clear_time();
                }
                _ => {}
            }
        }
        return;
    }

    if is_persisted_today_row_selected(app) {
        app.enter_delete_confirm(app::DeleteOrigin::Timer);
        return;
    }

    enqueue_action(action_tx, Action::StopServerTimerAndClear);
}
