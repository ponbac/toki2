use crate::app::{self, App, TextInput};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::action_queue::{Action, ActionTx};
use super::enqueue_action;

pub(super) fn handle_edit_description_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    let was_in_edit_mode = app.is_in_edit_mode();

    // CWD change mode takes priority.
    if app.cwd_input.is_some() {
        match key.code {
            KeyCode::Esc => app.cancel_cwd_change(),
            KeyCode::Enter => {
                if let Err(e) = app.confirm_cwd_change() {
                    app.status_message = Some(e);
                }
            }
            KeyCode::Tab => app.cwd_tab_complete(),
            KeyCode::Backspace => app.cwd_input_backspace(),
            KeyCode::Left => app.cwd_move_cursor(true),
            KeyCode::Right => app.cwd_move_cursor(false),
            KeyCode::Home => app.cwd_cursor_home_end(true),
            KeyCode::End => app.cwd_cursor_home_end(false),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.cwd_input_char(c);
            }
            _ => {}
        }
    } else if app.taskwarrior_overlay.is_some() {
        match key.code {
            KeyCode::Esc => app.close_taskwarrior_overlay(),
            KeyCode::Char('t') | KeyCode::Char('T')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.close_taskwarrior_overlay();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.taskwarrior_move(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.taskwarrior_move(false);
            }
            KeyCode::Enter => app.taskwarrior_confirm(),
            _ => {}
        }
    } else if app.git_mode {
        // Second key of Ctrl+G sequence.
        match key.code {
            KeyCode::Char('b') | KeyCode::Char('B') => app.paste_git_branch_raw(),
            KeyCode::Char('p') | KeyCode::Char('P') => app.paste_git_branch_parsed(),
            KeyCode::Char('c') | KeyCode::Char('C') => app.paste_git_last_commit(),
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
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && app.git_context.branch.is_some() =>
            {
                app.enter_git_mode();
            }
            KeyCode::Char('d') | KeyCode::Char('D')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.begin_cwd_change();
            }
            KeyCode::Char('t') | KeyCode::Char('T')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                app.open_taskwarrior_overlay();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.input_char(c);
            }
            KeyCode::Backspace => app.input_backspace(),
            KeyCode::Left => app.input_move_cursor(true),
            KeyCode::Right => app.input_move_cursor(false),
            KeyCode::Home => app.input_cursor_home_end(true),
            KeyCode::End => app.input_cursor_home_end(false),
            KeyCode::Enter => {
                if was_in_edit_mode {
                    app.update_edit_state_note(app.description_input.value.clone());
                    if let Some(saved_note) = app.saved_timer_note.take() {
                        app.description_input = TextInput::from_str(&saved_note);
                    }
                    let return_view = app.get_return_view_from_edit();
                    app.navigate_to(return_view);
                    if return_view == app::View::Timer {
                        app.focused_box = app::FocusedBox::Today;
                    }
                } else {
                    let should_sync_running_note = app.timer_state == app::TimerState::Running;
                    let note = app.description_input.value.clone();
                    app.confirm_description();
                    if should_sync_running_note {
                        enqueue_action(action_tx, Action::SyncRunningTimerNote { note });
                    }
                }
            }
            KeyCode::Esc => {
                if was_in_edit_mode {
                    if let Some(saved_note) = app.saved_timer_note.take() {
                        app.description_input = TextInput::from_str(&saved_note);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{EntryEditField, EntryEditState, TimerState, View};
    use crate::config::TokiConfig;
    use crossterm::event::{KeyEvent, KeyModifiers};

    use super::super::super::action_queue::channel;

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    fn test_app() -> App {
        let mut app = App::new(1, &TokiConfig::default());
        app.current_view = View::EditDescription;
        app.editing_description = true;
        app.description_is_default = false;
        app
    }

    fn trigger_enter(app: &mut App) -> Option<Action> {
        let (tx, mut rx) = channel();
        handle_edit_description_key(enter_key(), app, &tx);
        rx.try_recv().ok()
    }

    fn assert_sync_action(action: Option<Action>, expected_note: &str) {
        match action {
            Some(Action::SyncRunningTimerNote { note }) => assert_eq!(note, expected_note),
            Some(other) => panic!("unexpected action: {other:?}"),
            None => panic!("expected queued action"),
        }
    }

    #[test]
    fn enter_syncs_running_timer_note_clear() {
        let mut app = test_app();
        app.timer_state = TimerState::Running;
        app.description_input = TextInput::from_str("");

        assert_sync_action(trigger_enter(&mut app), "");
    }

    #[test]
    fn enter_in_edit_mode_updates_edit_state_without_sync() {
        let mut app = test_app();
        app.timer_state = TimerState::Running;
        app.saved_timer_note = Some("original running note".to_string());
        app.description_input = TextInput::from_str("entry note");
        app.this_week_edit_state = Some(EntryEditState {
            registration_id: "reg-1".to_string(),
            start_time_input: "09:00".to_string(),
            end_time_input: "10:00".to_string(),
            original_start_time: "09:00".to_string(),
            original_end_time: "10:00".to_string(),
            project_id: None,
            project_name: None,
            activity_id: None,
            activity_name: None,
            note: TextInput::from_str("before"),
            focused_field: EntryEditField::Note,
            validation_error: None,
        });
        let action = trigger_enter(&mut app);

        let note = &app
            .this_week_edit_state
            .as_ref()
            .expect("edit state should exist")
            .note
            .value;
        assert_eq!(note, "entry note");
        assert_eq!(app.description_input.value, "original running note");
        assert!(action.is_none(), "expected no queued action");
    }
}
