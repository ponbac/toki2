use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent};

use super::super::action_queue::{Action, ActionTx};
use super::enqueue_action;

pub(super) fn handle_confirm_delete_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            enqueue_action(action_tx, Action::ConfirmDelete);
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            let origin = app.delete_context.as_ref().map(|c| c.origin);
            app.delete_context = None;
            match origin {
                Some(app::DeleteOrigin::Timer) | None => app.navigate_to(app::View::Timer),
                Some(app::DeleteOrigin::History) => app.navigate_to(app::View::History),
            }
        }
        _ => {}
    }
}
