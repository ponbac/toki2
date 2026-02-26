use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "toki-tui")]
#[command(about = "Terminal UI for Toki time tracking")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run against a real toki-api server
    Run,
    /// Run in dev mode with local in-memory data
    Dev,
    /// Authenticate via browser OAuth login
    Login,
    /// Remove local session and Milltime cookies
    Logout,
    /// Print config path and create default file if missing
    ConfigPath,
}
