use std::{env, net::SocketAddr, time::Duration};

use domain::RepoConfig;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use tokio::net::TcpListener;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{app_state::AppState, config::read_config};

mod adapters;
mod app_state;
mod auth;
mod config;
mod domain;
mod factory;
mod repositories;
mod router;
mod routes;
mod utils;

#[tokio::main]
async fn main() {
    // Load environment variables and initialize tracing
    #[cfg(debug_assertions)]
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
    env::var("MILLTIME_URL").expect("MILLTIME_URL not set");
    env::var("MT_CRYPTO_KEY").expect("MT_CRYPTO_KEY not set");
    let config = read_config().expect("Failed to read configuration");
    let mut connection_pool_result = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(config.database.with_db())
        .await;
    if let Err(err) = connection_pool_result {
        tracing::error!("Failed to connect to database: {}", err);
        // tracing::error!("Config: {:?}", config.database);

        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
        // tracing::info!(
        //     "Trying to connect to database using DATABASE_URL: {}",
        //     db_url
        // );
        let pg_connect_options: PgConnectOptions = db_url.parse().unwrap();
        connection_pool_result = PgPoolOptions::new().connect_with(pg_connect_options).await;
    }
    let connection_pool = connection_pool_result.expect("Failed to connect to database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to run migrations");

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

async fn query_repository_configs(pool: &PgPool) -> Result<Vec<RepoConfig>, sqlx::Error> {
    let repos = sqlx::query_as!(
        RepoConfig,
        r#"
        SELECT organization, project, repo_name, token
        FROM repositories
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(repos)
}
