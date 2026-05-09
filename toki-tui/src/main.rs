mod api;
mod app;
mod bootstrap;
mod cli;
mod config;
mod editor;
mod git;
mod log_notes;
mod login;
mod runtime;
mod session_store;
mod terminal;
#[cfg(test)]
mod test_support;
mod time_utils;
mod types;
mod ui;

use anyhow::Result;
use api::ApiClient;
use app::App;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ConfigPath => {
            let path = config::TokiConfig::ensure_exists()?;
            println!("{}", path.display());
        }
        Commands::LogsPath => {
            let path = log_notes::log_dir()?;
            println!("{}", path.display());
        }
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Status => {
            let session = session_store::load_session()?;
            let session_status = if session.is_some() {
                "logged in"
            } else {
                "not logged in"
            };
            println!("Azure AD: {}", session_status);
        }
        Commands::Login => {
            let cfg = config::TokiConfig::load()?;
            login::run_login(&cfg.api_url).await?;
        }
        Commands::Logout => {
            session_store::clear_session()?;
            println!("Logged out. Session cleared.");
        }
        Commands::Dev => {
            run_dev_mode().await?;
        }
        Commands::Run => {
            run_real_mode().await?;
        }
    }

    Ok(())
}

async fn run_dev_mode() -> Result<()> {
    let cfg = config::TokiConfig::load()?;
    let mut client = ApiClient::dev()?;
    let me = client.me().await?;

    println!("Dev mode: logged in as {} ({})\n", me.full_name, me.email);
    run_ui(App::new(me.id, &cfg), client).await
}

async fn run_real_mode() -> Result<()> {
    let cfg = config::TokiConfig::load()?;

    let session_id = match session_store::load_session()? {
        Some(session_id) => session_id,
        None => {
            anyhow::bail!("Not logged in. Run `toki-tui login` to authenticate.");
        }
    };

    let mut client = ApiClient::new(&cfg.api_url, &session_id)?;

    let me = client.me().await?;
    println!("Logged in as {} ({})\n", me.full_name, me.email);

    run_ui(App::new(me.id, &cfg), client).await
}

async fn run_ui(mut app: App, mut client: ApiClient) -> Result<()> {
    bootstrap::initialize_app_state(&mut app, &mut client).await;

    let result = {
        let mut terminal = terminal::TerminalGuard::new()?;
        runtime::run_app(terminal.terminal_mut(), &mut app, &mut client).await
    };

    if let Err(err) = result {
        eprintln!("Error: {:?}", err);
    }

    println!("\nGoodbye!");
    Ok(())
}
