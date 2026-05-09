use crate::app::{self, App};
use crossterm::event::KeyEvent;

use super::action_queue::{Action, ActionTx};

mod confirm_delete;
mod edit_description;
mod history;
mod save_action;
mod selection;
mod statistics;
mod template_selection;
mod timer;

fn enqueue_action(action_tx: &ActionTx, action: Action) {
    let _ = action_tx.send(action);
}

pub(super) fn handle_view_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match &app.current_view {
        app::View::SelectProject => selection::handle_select_project_key(key, app, action_tx),
        app::View::SelectActivity => selection::handle_select_activity_key(key, app, action_tx),
        app::View::SelectTemplate => {
            template_selection::handle_select_template_key(key, app, action_tx)
        }
        app::View::EditDescription => {
            edit_description::handle_edit_description_key(key, app, action_tx)
        }
        app::View::SaveAction => save_action::handle_save_action_key(key, app, action_tx),
        app::View::History => history::handle_history_key(key, app, action_tx),
        app::View::Statistics => statistics::handle_statistics_key(key, app),
        app::View::ConfirmDelete => confirm_delete::handle_confirm_delete_key(key, app, action_tx),
        app::View::Timer => timer::handle_timer_key(key, app, action_tx),
    }
}
