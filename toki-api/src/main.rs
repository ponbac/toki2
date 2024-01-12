use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use az_devops::{PullRequest, RepoClient};
use serde::Deserialize;
use tokio::net::TcpListener;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::{instrument, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::read_config;

mod config;

#[derive(Clone)]
struct AppState {
    repo_clients: Arc<HashMap<String, RepoClient>>,
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

    let organization = env::var("ADO_ORGANIZATION").unwrap();
    let project = env::var("ADO_PROJECT").unwrap();
    let repo_name = env::var("ADO_REPO").unwrap();
    let token = env::var("ADO_TOKEN").unwrap();

    // let organization_2 = env::var("ADO_ORGANIZATION_2").unwrap();
    // let project_2 = env::var("ADO_PROJECT_2").unwrap();
    // let repo_name_2 = env::var("ADO_REPO_2").unwrap();
    // let token_2 = env::var("ADO_TOKEN_2").unwrap();

    let repo_client = RepoClient::new(&repo_name, &organization, &project, &token)
        .await
        .unwrap();
    // let repo_client_2 = RepoClient::new(&repo_name_2, &organization_2, &project_2, &token_2)
    //     .await
    //     .unwrap();

    let config = read_config().expect("Failed to read configuration");

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(open_pull_requests))
        .with_state(AppState {
            repo_clients: Arc::new(
                vec![
                    (repo_name.to_lowercase(), repo_client),
                    // (repo_name_2.to_lowercase(), repo_client_2),
                ]
                .into_iter()
                .collect(),
            ),
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
    let client = app_state
        .repo_clients
        .get(&query.repo_name.to_lowercase())
        .ok_or((
            StatusCode::NOT_FOUND,
            format!(
                "Repository '{}' not found. Available repositories: [{}]",
                query.repo_name,
                app_state
                    .repo_clients
                    .keys()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
        ))?;

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
