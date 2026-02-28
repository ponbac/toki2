mod api;
mod app;
mod bootstrap;
mod cli;
mod config;
mod git;
mod login;
mod runtime;
mod session_store;
mod terminal;
mod time_utils;
mod types;
mod ui;

use anyhow::Result;
use api::ApiClient;
use app::App;
use clap::Parser;
use cli::{Cli, Commands};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ConfigPath => {
            let path = config::TokiConfig::ensure_exists()?;
            println!("{}", path.display());
        }
        Commands::Login => {
            let cfg = config::TokiConfig::load()?;
            login::run_login(&cfg.api_url).await?;
        }
        Commands::Logout => {
            session_store::clear_session()?;
            session_store::clear_mt_cookies()?;
            println!("Logged out. Session and Milltime cookies cleared.");
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

    let mt_cookies = session_store::load_mt_cookies()?;
    let mut client = ApiClient::new(&cfg.api_url, &session_id, mt_cookies)?;

    let me = client.me().await?;
    println!("Logged in as {} ({})\n", me.full_name, me.email);

    if client.mt_cookies().is_empty() {
        prompt_milltime_authentication(&mut client).await?;
    }

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

async fn prompt_milltime_authentication(client: &mut ApiClient) -> Result<()> {
    println!("Milltime credentials required.");
    print!("Username: ");
    std::io::stdout().flush()?;

    let mut username = String::new();
    std::io::stdin().read_line(&mut username)?;
    let username = username.trim().to_string();

    let password = rpassword::prompt_password("Password: ")?;

    print!("Authenticating...");
    std::io::stdout().flush()?;
    client.authenticate(&username, &password).await?;
    println!(" OK");

    Ok(())
}
