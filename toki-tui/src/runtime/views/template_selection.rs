use crate::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::action_queue::{Action, ActionTx};
use super::enqueue_action;

pub(super) fn handle_select_template_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    if handle_template_input_key(key, app) {
        return;
    }

    match key.code {
        KeyCode::Enter => {
            if let Some(template) = app
                .filtered_templates
                .get(app.filtered_template_index)
                .cloned()
            {
                enqueue_action(action_tx, Action::ApplyTemplate { template });
            } else {
                app.navigate_to(crate::app::View::Timer);
            }
        }
        KeyCode::Esc => app.navigate_to(crate::app::View::Timer),
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        _ => {}
    }
}

fn handle_template_input_key(key: KeyEvent, app: &mut App) -> bool {
    let list_index = app.filtered_template_index;
    let list_len = app.filtered_templates.len();

    match key.code {
        KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.template_search_input_clear();
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
                app.template_search_input_char(c);
            }
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
            app.template_search_delete_word_back();
            true
        }
        KeyCode::Backspace => {
            app.template_search_input_backspace();
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
        KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.selection_list_focused {
                app.template_search_word_left();
            }
            true
        }
        KeyCode::Left => {
            if !app.selection_list_focused {
                app.template_search_move_cursor(true);
            }
            true
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if !app.selection_list_focused {
                app.template_search_word_right();
            }
            true
        }
        KeyCode::Right => {
            if !app.selection_list_focused {
                app.template_search_move_cursor(false);
            }
            true
        }
        KeyCode::Home => {
            if !app.selection_list_focused {
                app.template_search_cursor_home_end(true);
            }
            true
        }
        KeyCode::End => {
            if !app.selection_list_focused {
                app.template_search_cursor_home_end(false);
            }
            true
        }
        _ => false,
    }
}
