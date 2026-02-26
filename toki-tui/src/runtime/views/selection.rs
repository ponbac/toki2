use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::action_queue::{Action, ActionTx};
use super::enqueue_action;

pub(super) fn handle_select_project_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    // Save edit state and running timer state before any selection.
    let had_edit_state = app.is_in_edit_mode();
    let saved_selected_project = app.selected_project.clone();
    let saved_selected_activity = app.selected_activity.clone();

    if handle_selection_input_key(
        key,
        app,
        app.filtered_project_index,
        app.filtered_projects.len(),
        SelectionInputOps {
            clear_input: App::search_input_clear,
            input_char: App::search_input_char,
            input_backspace: App::search_input_backspace,
            move_cursor: App::search_move_cursor,
            cursor_home_end: App::search_cursor_home_end,
        },
    ) {
        return;
    }

    match key.code {
        KeyCode::Enter => {
            app.confirm_selection();
            enqueue_action(
                action_tx,
                Action::ApplyProjectSelection {
                    had_edit_state,
                    saved_selected_project,
                    saved_selected_activity,
                },
            );
        }
        KeyCode::Esc => app.cancel_selection(),
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        _ => {}
    }
}

pub(super) fn handle_select_activity_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    // Save edit state and running timer state before any selection.
    let was_in_edit_mode = app.is_in_edit_mode();
    let saved_selected_project = app.selected_project.clone();
    let saved_selected_activity = app.selected_activity.clone();

    if handle_selection_input_key(
        key,
        app,
        app.filtered_activity_index,
        app.filtered_activities.len(),
        SelectionInputOps {
            clear_input: App::activity_search_input_clear,
            input_char: App::activity_search_input_char,
            input_backspace: App::activity_search_input_backspace,
            move_cursor: App::activity_search_move_cursor,
            cursor_home_end: App::activity_search_cursor_home_end,
        },
    ) {
        return;
    }

    match key.code {
        KeyCode::Enter => {
            app.confirm_selection();
            enqueue_action(
                action_tx,
                Action::ApplyActivitySelection {
                    was_in_edit_mode,
                    saved_selected_project,
                    saved_selected_activity,
                },
            );
        }
        KeyCode::Esc => app.cancel_selection(),
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        _ => {}
    }
}

fn handle_selection_input_key(
    key: KeyEvent,
    app: &mut App,
    list_index: usize,
    list_len: usize,
    ops: SelectionInputOps,
) -> bool {
    match key.code {
        KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            (ops.clear_input)(app);
            true
        }
        KeyCode::Tab => {
            app.selection_list_focused = true;
            true
        }
        KeyCode::BackTab => {
            app.selection_list_focused = false;
            true
        }
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL) && c != 'q' && c != 'Q' =>
        {
            if app.selection_list_focused && c == 'j' {
                if list_index + 1 >= list_len {
                    app.selection_list_focused = false;
                } else {
                    app.select_next();
                }
            } else if app.selection_list_focused && c == 'k' {
                if list_index == 0 {
                    app.selection_list_focused = false;
                } else {
                    app.select_previous();
                }
            } else if !app.selection_list_focused {
                (ops.input_char)(app, c);
            }
            true
        }
        KeyCode::Backspace => {
            (ops.input_backspace)(app);
            true
        }
        KeyCode::Up => {
            if app.selection_list_focused && list_index == 0 {
                app.selection_list_focused = false;
            } else {
                app.select_previous();
            }
            true
        }
        KeyCode::Down => {
            if app.selection_list_focused && list_index + 1 >= list_len {
                app.selection_list_focused = false;
            } else {
                app.select_next();
            }
            true
        }
        KeyCode::Left => {
            if !app.selection_list_focused {
                (ops.move_cursor)(app, true);
            }
            true
        }
        KeyCode::Right => {
            if !app.selection_list_focused {
                (ops.move_cursor)(app, false);
            }
            true
        }
        KeyCode::Home => {
            if !app.selection_list_focused {
                (ops.cursor_home_end)(app, true);
            }
            true
        }
        KeyCode::End => {
            if !app.selection_list_focused {
                (ops.cursor_home_end)(app, false);
            }
            true
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct SelectionInputOps {
    clear_input: fn(&mut App),
    input_char: fn(&mut App, char),
    input_backspace: fn(&mut App),
    move_cursor: fn(&mut App, bool),
    cursor_home_end: fn(&mut App, bool),
}
