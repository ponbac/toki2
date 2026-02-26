use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent};

use super::super::action_queue::{Action, ActionTx};
use super::enqueue_action;

pub(super) fn handle_save_action_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match key.code {
        KeyCode::Char('1') => {
            app.select_save_action_by_number(1);
            enqueue_action(action_tx, Action::SaveTimer);
        }
        KeyCode::Char('2') => {
            app.select_save_action_by_number(2);
            enqueue_action(action_tx, Action::SaveTimer);
        }
        KeyCode::Char('3') => {
            app.select_save_action_by_number(3);
            enqueue_action(action_tx, Action::SaveTimer);
        }
        KeyCode::Char('4') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
            app.navigate_to(app::View::Timer);
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_previous_save_action(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next_save_action(),
        KeyCode::Enter => {
            enqueue_action(action_tx, Action::SaveTimer);
        }
        _ => {}
    }
}
