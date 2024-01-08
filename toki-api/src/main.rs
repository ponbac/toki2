use std::{env, sync::Arc};

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use az_devops::RepoClient;
use tokio::net::TcpListener;

#[derive(Clone)]
struct AppState {
    repo_client: Arc<RepoClient>,
}

#[tokio::main]
async fn main() {
    dotenvy::from_filename("./toki-api/.env.local").ok();

    let organization = env::var("ADO_ORGANIZATION").unwrap();
    let project = env::var("ADO_PROJECT").unwrap();
    let repo_name = env::var("ADO_REPO").unwrap();
    let token = env::var("ADO_TOKEN").unwrap();

    let repo_client = RepoClient::new(&repo_name, &organization, &project, &token)
        .await
        .unwrap();

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(open_pull_requests))
        .with_state(AppState {
            repo_client: Arc::new(repo_client),
        });

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn open_pull_requests(State(app_state): State<AppState>) -> impl IntoResponse {
    let pull_requests = app_state
        .repo_client
        .get_open_pull_requests()
        .await
        .unwrap();

    Json(pull_requests)
}
