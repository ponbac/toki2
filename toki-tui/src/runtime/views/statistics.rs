use crate::app::{self, App};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_statistics_key(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Esc => {
            app.navigate_to(app::View::Timer);
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        _ => {}
    }
}
