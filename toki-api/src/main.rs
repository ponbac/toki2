use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use az_devops::{PullRequest, RepoClient};
use repository::insert_repository;
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::{net::TcpListener, sync::Mutex};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{instrument, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::read_config,
    repository::{query_repositories, repo_configs_to_clients},
};

mod config;
mod repository;

#[derive(Clone)]
struct AppState {
    db_pool: Arc<PgPool>,
    repo_clients: Arc<Mutex<HashMap<String, RepoClient>>>,
}

#[tokio::main]
async fn main() {
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

    let config = read_config().expect("Failed to read configuration");
    let connection_pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(config.database.with_db())
        .await
        .expect("Failed to connect to database");

    let repo_configs = query_repositories(&connection_pool)
        .await
        .expect("Failed to query repos");
    tracing::info!("Found {} repositories: [{}]", repo_configs.len(), repo_configs
        .iter()
        .map(|repo| format!("{} ({}/{})", repo.repo_name, repo.organization, repo.project))
        .collect::<Vec<String>>()
        .join(", "));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(open_pull_requests))
        .route("/repositories", post(add_repository))
        .with_state(AppState {
            db_pool: Arc::new(connection_pool),
            repo_clients: Arc::new(Mutex::new(
                repo_configs_to_clients(repo_configs)
                    .await
                    .expect("Failed to create repo clients"),
            )),
        })
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default()));

    let socket_addr = format!("{}:{}", config.application.host, config.application.port)
        .parse::<SocketAddr>()
        .expect("Failed to parse socket address");

    tracing::info!("Starting server at {}", socket_addr);
    let listener = TcpListener::bind(socket_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPullRequestsQuery {
    repo_name: String,
    author: Option<String>,
}

#[instrument(
    name = "GET /pull-requests",
    skip(app_state, query),
    fields(
        repo_name = %query.repo_name,
        author = ?query.author,
    )
)]
async fn open_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<OpenPullRequestsQuery>,
) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
    let repos_map = app_state.repo_clients.lock().await;
    let client = repos_map
        .get(&query.repo_name.to_lowercase())
        .ok_or((
            StatusCode::NOT_FOUND,
            format!(
                "Repository '{}' not found. Available repositories: [{}]",
                query.repo_name,
                repos_map
                    .keys()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        ))?
        .clone();
    drop(repos_map);

    let pull_requests = client
        .get_open_pull_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|pr| {
            if let Some(author) = &query.author {
                pr.created_by.unique_name == *author
            } else {
                true
            }
        })
        .collect::<Vec<PullRequest>>();
    tracing::debug!(
        "Found {} open pull requests: [{}]",
        pull_requests.len(),
        pull_requests
            .iter()
            .map(|pr| pr.title.clone())
            .collect::<Vec<String>>()
            .join(", ")
    );

    Ok(Json(pull_requests))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRepositoryBody {
    organization: String,
    project: String,
    repo_name: String,
    token: String,
}

#[instrument(
    name = "POST /repositories",
    skip(app_state, body), 
    fields(
        organization = %body.organization,
        project = %body.project,
        repo_name = %body.repo_name,
    )
)]
async fn add_repository(
    State(app_state): State<AppState>,
    Json(body): Json<AddRepositoryBody>,
) -> Result<String, (StatusCode, String)> {
    let repo_client = RepoClient::new(
        &body.repo_name,
        &body.organization,
        &body.project,
        &body.token,
    )
    .await
    .map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create repository: {}", err),
        )
    })?;

    let db_id = insert_repository(
        &app_state.db_pool,
        &body.organization,
        &body.project,
        &body.repo_name,
        &body.token,
    )
    .await
    .map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to insert repository: {}", err),
        )
    })?;

    app_state
        .repo_clients
        .lock()
        .await
        .insert(body.repo_name.to_lowercase(), repo_client);
    tracing::info!("Added new repository: {}", body.repo_name);

    Ok(db_id.to_string())
}
