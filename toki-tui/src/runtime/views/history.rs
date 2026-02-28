use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::action_queue::{Action, ActionTx};
use super::super::actions::handle_entry_edit_enter;
use super::enqueue_action;

pub(super) fn handle_history_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    // Check if we're in edit mode.
    if app.history_edit_state.is_some() {
        match key.code {
            KeyCode::Tab => {
                app.entry_edit_next_field();
            }
            KeyCode::BackTab => {
                app.entry_edit_prev_field();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.entry_edit_next_field();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.entry_edit_prev_field();
            }
            KeyCode::Right => {
                if app
                    .history_edit_state
                    .as_ref()
                    .is_some_and(|s| s.focused_field == app::EntryEditField::Note)
                {
                    app.entry_edit_move_cursor(false);
                } else {
                    app.entry_edit_next_field();
                }
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.entry_edit_next_field();
            }
            KeyCode::Left => {
                if app
                    .history_edit_state
                    .as_ref()
                    .is_some_and(|s| s.focused_field == app::EntryEditField::Note)
                {
                    app.entry_edit_move_cursor(true);
                } else {
                    app.entry_edit_prev_field();
                }
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.entry_edit_prev_field();
            }
            KeyCode::Home => app.entry_edit_cursor_home_end(true),
            KeyCode::End => app.entry_edit_cursor_home_end(false),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                app.entry_edit_input_char(c);
            }
            KeyCode::Backspace => {
                app.entry_edit_backspace();
            }
            KeyCode::Enter => {
                if let Some(state) = &app.history_edit_state {
                    match state.focused_field {
                        app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                            app.entry_edit_next_field();
                        }
                        _ => {
                            handle_entry_edit_enter(app);
                        }
                    }
                }
            }
            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(state) = &app.history_edit_state {
                    match state.focused_field {
                        app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                            app.entry_edit_clear_time();
                        }
                        _ => {}
                    }
                }
            }
            // Escape: save and exit edit mode.
            KeyCode::Esc => {
                if let Some(error) = app.entry_edit_validate() {
                    app.entry_edit_revert_invalid_times();
                    app.set_status(format!("Edit cancelled: {}", error));
                    app.exit_history_edit_mode();
                } else {
                    enqueue_action(action_tx, Action::SaveHistoryEdit);
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.navigate_to(app::View::SelectProject);
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.quit();
            }
            _ => {}
        }
    } else {
        // Not in edit mode.
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
            KeyCode::Enter => {
                app.enter_history_edit_mode();
            }
            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Esc => {
                app.navigate_to(app::View::Timer);
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
            KeyCode::Delete | KeyCode::Backspace if app.focused_history_index.is_some() => {
                app.enter_delete_confirm(app::DeleteOrigin::History);
            }
            KeyCode::Char('x')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && app.focused_history_index.is_some() =>
            {
                app.enter_delete_confirm(app::DeleteOrigin::History);
            }
            _ => {}
        }
    }
}
