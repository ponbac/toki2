use std::{net::SocketAddr, time::Duration};

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    app_state::AppState,
    config::read_config,
    repository::{query_repository_configs, repo_configs_to_clients},
};

mod app_state;
mod config;
mod repository;
mod routes;

#[tokio::main]
async fn main() {
    // Load environment variables and initialize tracing
    dotenvy::from_filename("./toki-api/.env.local").ok();
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::DEBUG.into())
                .from_env_lossy()
                .add_directive("hyper=info".parse().unwrap()),
        )
        .init();

    // Read the configuration and connect to the database
    let config = read_config().expect("Failed to read configuration");
    let connection_pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(config.database.with_db())
        .await
        .expect("Failed to connect to database");

    // Fetch all repositories from the database and create a client for each one
    let repo_configs = query_repository_configs(&connection_pool)
        .await
        .expect("Failed to query repos");
    tracing::info!(
        "Found {} repositories: [{}]",
        repo_configs.len(),
        repo_configs
            .iter()
            .map(|repo| format!(
                "{} ({}/{})",
                repo.repo_name, repo.organization, repo.project
            ))
            .collect::<Vec<String>>()
            .join(", ")
    );

    // Create the router and start the server
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(routes::open_pull_requests))
        .route("/repositories", get(routes::get_repositories))
        .route("/repositories", post(routes::add_repository))
        .with_state(AppState::new(
            connection_pool,
            repo_configs_to_clients(repo_configs)
                .await
                .expect("Failed to create repo clients"),
        ))
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default()));

    let socket_addr = format!("{}:{}", config.application.host, config.application.port)
        .parse::<SocketAddr>()
        .expect("Failed to parse socket address");

    tracing::info!("Starting server at {}", socket_addr);
    let listener = TcpListener::bind(socket_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
