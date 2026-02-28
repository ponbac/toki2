use crate::api::ApiClient;
use crate::app::App;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, Instant};

use super::action_queue::{channel, Action};
use super::actions::run_action;
use super::views::{handle_milltime_reauth_key, handle_view_key};

pub async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    // Show throbber for at least 3 seconds on startup.
    app.is_loading = true;
    let loading_until = Instant::now() + Duration::from_secs(3);

    // Background polling: refresh time entries every 60 seconds.
    let mut last_history_refresh = Instant::now();
    const HISTORY_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

    let (action_tx, mut action_rx) = channel();

    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if app.is_loading {
            app.throbber_state.calc_next();
            if Instant::now() >= loading_until {
                app.is_loading = false;
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if app.milltime_reauth.is_some() {
                    handle_milltime_reauth_key(key, app, &action_tx);
                } else {
                    handle_view_key(key, app, &action_tx);
                }
            }
        }

        if last_history_refresh.elapsed() >= HISTORY_REFRESH_INTERVAL && !app.is_in_edit_mode() {
            let _ = action_tx.send(Action::RefreshHistoryBackground);
            last_history_refresh = Instant::now();
        }

        while let Ok(action) = action_rx.try_recv() {
            run_action(action, app, client).await?;
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
