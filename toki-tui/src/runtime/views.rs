use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::action_queue::{Action, ActionTx};

mod confirm_delete;
mod edit_description;
mod history;
mod save_action;
mod selection;
mod statistics;
mod timer;

fn enqueue_action(action_tx: &ActionTx, action: Action) {
    let _ = action_tx.send(action);
}

pub(super) fn handle_milltime_reauth_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            app.milltime_reauth_next_field();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.milltime_reauth_input_char(c);
        }
        KeyCode::Backspace => {
            app.milltime_reauth_backspace();
        }
        KeyCode::Enter => {
            enqueue_action(action_tx, Action::SubmitMilltimeReauth);
        }
        KeyCode::Esc => {
            app.close_milltime_reauth();
            app.set_status("Milltime re-authentication cancelled".to_string());
        }
        _ => {}
    }
}

pub(super) fn handle_view_key(key: KeyEvent, app: &mut App, action_tx: &ActionTx) {
    match &app.current_view {
        app::View::SelectProject => selection::handle_select_project_key(key, app, action_tx),
        app::View::SelectActivity => selection::handle_select_activity_key(key, app, action_tx),
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
