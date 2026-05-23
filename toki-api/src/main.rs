use std::{env, net::SocketAddr, time::Duration};

use domain::RepoConfig;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tokio::{net::TcpListener, signal};
use tracing::Instrument;

use crate::{app_state::AppState, config::read_config};

mod adapters;
mod app_state;
mod auth;
mod config;
mod db;
mod domain;
mod factory;
mod observability;
mod repositories;
mod router;
mod routes;
mod utils;

#[tokio::main]
async fn main() {
    // Load environment variables and initialize tracing
    #[cfg(debug_assertions)]
    {
        dotenvy::from_filename("./toki-api/.env.local")
            .or_else(|_| dotenvy::from_filename(".env.local"))
            .ok();
    }

    // Read the configuration and connect to the database
    let config = read_config().expect("Failed to read configuration");
    let observability_guard =
        observability::init(&config).expect("Failed to initialize observability");
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
    let db_pool = db::traced_pool(connection_pool.clone(), &config.database);

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .instrument(observability::db_operation_span(
            &config.database,
            "MIGRATE",
            r#"sqlx::migrate!("./migrations").run(...)"#,
        ))
        .await
        .expect("Failed to run migrations");

    // Fetch all repositories from the database
    let repo_configs = query_repository_configs(&db_pool)
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
    let app = router::create(
        connection_pool.clone(),
        db_pool,
        repo_configs,
        config.clone(),
    )
    .await;
    let socket_addr = format!("{}:{}", config.application.host, config.application.port)
        .parse::<SocketAddr>()
        .expect("Failed to parse socket address");

    tracing::info!("Starting server at {}", socket_addr);
    let listener = TcpListener::bind(socket_addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    observability_guard.shutdown();
}

async fn query_repository_configs(pool: &db::DbPool) -> Result<Vec<RepoConfig>, sqlx::Error> {
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

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install terminate signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
