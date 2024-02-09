use std::{net::SocketAddr, time::Duration};

use domain::RepoConfig;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{app_state::AppState, config::read_config};

mod app_state;
mod auth;
mod config;
mod domain;
mod repositories;
mod router;
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
                .add_directive("hyper=info".parse().unwrap())
                .add_directive("azure_core::policies::transport=info".parse().unwrap()),
        )
        .init();

    // Read the configuration and connect to the database
    let config = read_config().expect("Failed to read configuration");
    let connection_pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(config.database.with_db())
        .await
        .expect("Failed to connect to database");

    // Fetch all repositories from the database
    let repo_configs = query_repository_configs(&connection_pool)
        .await
        .expect("Failed to query repos");
    tracing::info!(
        "Found {} repositories: [{}]",
        repo_configs.len(),
        repo_configs
            .iter()
            .map(|repo| repo.key().to_string())
            .collect::<Vec<String>>()
            .join(", ")
    );

    // Create the router and start the server
    let app = router::create(connection_pool.clone(), repo_configs, config.clone()).await;
    let socket_addr = format!("{}:{}", config.application.host, config.application.port)
        .parse::<SocketAddr>()
        .expect("Failed to parse socket address");

    tracing::info!("Starting server at {}", socket_addr);
    let listener = TcpListener::bind(socket_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn query_repository_configs(
    pool: &PgPool,
) -> Result<Vec<RepoConfig>, Box<dyn std::error::Error>> {
    let repos = sqlx::query_as!(
        RepoConfig,
        r#"
        SELECT id, organization, project, repo_name, token
        FROM repositories
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(repos)
}
