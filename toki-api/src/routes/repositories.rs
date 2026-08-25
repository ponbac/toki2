use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use axum_login::permission_required;
use az_devops::RepoClient;
use serde::{Deserialize, Serialize};

use crate::{
    auth::{AuthBackend, AuthUser},
    domain::{RepoDifferMessage, RepoKey, Repository, Role},
    observability::{record_repo_key, record_span_field, record_user_id},
    repositories::{NewRepository, RepoRepository, UserRepository},
    AppState,
};

use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", delete(delete_repository))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
        .route("/", get(get_repositories))
        .route("/", post(add_repository))
        .route("/follow", post(follow_repository))
}

async fn get_repositories(State(app_state): State<AppState>) -> Json<Vec<Repository>> {
    let repository_repo = app_state.repository_repo.clone();
    let repos = repository_repo
        .get_repositories()
        .await
        .expect("Failed to query repos");

    Json(repos)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FollowRepositoryBody {
    organization: String,
    project: String,
    repo_name: String,
    follow: bool,
}

async fn follow_repository(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<FollowRepositoryBody>,
) -> Result<Json<()>, ApiError> {
    record_user_id(user.id);
    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    record_repo_key(&repo_key);
    let user_repo = app_state.user_repo.clone();

    user_repo
        .follow_repository(user.id, &repo_key, body.follow)
        .await?;

    Ok(Json(()))
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

async fn add_repository(
    State(app_state): State<AppState>,
    Json(body): Json<AddRepositoryBody>,
) -> Result<Json<AddRepositoryResponse>, ApiError> {
    record_span_field("organization", &body.organization);
    record_span_field("project", &body.project);
    record_span_field("repo_name", &body.repo_name);
    let repo_client = RepoClient::new(
        &body.repo_name,
        &body.organization,
        &body.project,
        &body.token,
    )
    .await
    .map_err(|err| ApiError::bad_request(format!("Failed to create repository: {}", err)))?;

    let repository_repo = app_state.repository_repo.clone();
    let new_repo = NewRepository::new(
        body.organization.clone(),
        body.project.clone(),
        body.repo_name.clone(),
        body.token.clone(),
    );
    let id = repository_repo.upsert_repository(&new_repo).await?;

    let key = RepoKey::from(&body);
    record_repo_key(&key);
    app_state.insert_repo(repo_client).await;
    tracing::info!("Added new repository: {}", key);

    // start differ
    tokio::spawn(async move {
        let sender = app_state.get_differ_sender(key).await.unwrap();
        sender
            .send(RepoDifferMessage::Start(Duration::from_secs(300)))
            .await
            .unwrap();
    });

    Ok(Json(AddRepositoryResponse { id }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteRepositoryBody {
    organization: String,
    project: String,
    repo_name: String,
}

async fn delete_repository(
    State(app_state): State<AppState>,
    Json(body): Json<DeleteRepositoryBody>,
) -> Result<StatusCode, ApiError> {
    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    record_repo_key(&repo_key);
    let repository_repo = app_state.repository_repo.clone();

    repository_repo.delete_repository(&repo_key).await?;

    app_state.delete_repo(repo_key).await;

    Ok(StatusCode::OK)
}
