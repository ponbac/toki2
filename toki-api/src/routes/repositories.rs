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
use tracing::instrument;

use crate::{
    auth::{AuthBackend, AuthSession},
    domain::{RepoDifferMessage, RepoKey, Repository, Role},
    repositories::{NewRepository, RepoRepository, UserRepository},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", delete(delete_repository))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
        .route("/", get(get_repositories))
        .route("/", post(add_repository))
        .route("/follow", post(follow_repository))
        .route("/milltime-project", post(update_milltime_project))
}

#[instrument(name = "GET /repositories", skip(app_state))]
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

#[instrument(name = "POST /repositories/follow", skip(auth_session, app_state))]
async fn follow_repository(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<FollowRepositoryBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;

    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    let user_repo = app_state.user_repo.clone();

    user_repo
        .follow_repository(user_id, &repo_key, body.follow)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to follow repository: {}", err),
            )
        })?;

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

    let repository_repo = app_state.repository_repo.clone();
    let new_repo = NewRepository::new(
        body.organization.clone(),
        body.project.clone(),
        body.repo_name.clone(),
        body.token.clone(),
    );
    let id = repository_repo
        .upsert_repository(&new_repo)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert repository: {}", err),
            )
        })?;

    let key = RepoKey::from(&body);
    app_state.insert_repo(key.clone(), repo_client).await;
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

#[instrument(name = "DELETE /repositories", skip(app_state))]
async fn delete_repository(
    State(app_state): State<AppState>,
    Json(body): Json<DeleteRepositoryBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    let repository_repo = app_state.repository_repo.clone();

    repository_repo
        .delete_repository(&repo_key)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete repository: {}", err),
            )
        })?;

    app_state.delete_repo(repo_key).await;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMilltimeProjectBody {
    organization: String,
    project: String,
    repo_name: String,
    milltime_project_id: Option<String>,
}

#[instrument(name = "POST /repositories/milltime-project", skip(app_state))]
async fn update_milltime_project(
    State(app_state): State<AppState>,
    Json(body): Json<UpdateMilltimeProjectBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    let repository_repo = app_state.repository_repo.clone();

    repository_repo
        .update_milltime_project(&repo_key, body.milltime_project_id)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update Milltime project: {}", err),
            )
        })?;

    Ok(StatusCode::OK)
}
