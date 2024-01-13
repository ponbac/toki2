use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use az_devops::{PullRequest, RepoClient};
use repository::{insert_repository, query_repository_dtos, RepoKey, RepositoryDto};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::{net::TcpListener, sync::Mutex};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{instrument, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    config::read_config,
    repository::{query_repository_configs, repo_configs_to_clients},
};

mod config;
mod repository;

#[derive(Clone)]
struct AppState {
    db_pool: Arc<PgPool>,
    repo_clients: Arc<Mutex<HashMap<RepoKey, RepoClient>>>,
}

impl AppState {
    async fn get_repo_client(&self, key: impl Into<RepoKey>) -> Result<RepoClient, String> {
        let repo_clients = self.repo_clients.lock().await;
        let key: RepoKey = key.into();

        repo_clients
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("Repository '{}' not found", key))
    }

    async fn insert_repo_client(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let mut repo_clients = self.repo_clients.lock().await;
        let key: RepoKey = key.into();

        repo_clients.insert(key, client);
    }
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

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(open_pull_requests))
        .route("/repositories", get(get_repositories))
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPullRequestsQuery {
    organization: String,
    project: String,
    repo_name: String,
    author: Option<String>,
}

impl From<&OpenPullRequestsQuery> for RepoKey {
    fn from(query: &OpenPullRequestsQuery) -> Self {
        Self::new(&query.organization, &query.project, &query.repo_name)
    }
}

#[instrument(name = "GET /pull-requests", skip(app_state))]
async fn open_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<OpenPullRequestsQuery>,
) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
    let client = app_state
        .get_repo_client(&query)
        .await
        .map_err(|err| (StatusCode::NOT_FOUND, err))?;

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

async fn get_repositories(State(app_state): State<AppState>) -> Json<Vec<RepositoryDto>> {
    let repos = query_repository_dtos(&app_state.db_pool)
        .await
        .expect("Failed to query repos");

    Json(repos)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRepositoryBody {
    organization: String,
    project: String,
    repo_name: String,
    token: String,
}

impl From<&AddRepositoryBody> for RepoKey {
    fn from(body: &AddRepositoryBody) -> Self {
        Self::new(&body.organization, &body.project, &body.repo_name)
    }
}

#[derive(Debug, Serialize)]
struct AddRepositoryResponse {
    id: i32,
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
) -> Result<Json<AddRepositoryResponse>, (StatusCode, String)> {
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

    let id = insert_repository(
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

    app_state.insert_repo_client(&body, repo_client).await;
    tracing::info!("Added new repository: {}", body.repo_name);

    Ok(Json(AddRepositoryResponse { id }))
}
