use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use az_devops::RepoClient;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    auth::AuthSession,
    domain::{RepoKey, Repository},
    repositories::{NewRepository, RepoRepository, UserRepository},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_repositories))
        .route("/", post(add_repository))
        .route("/follow", post(follow_repository))
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

#[instrument(
    name = "POST /repositories/follow",
    skip(auth_session, app_state, body)
)]
async fn follow_repository(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<FollowRepositoryBody>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;

    let repo_key = RepoKey::new(&body.organization, &body.project, &body.repo_name);
    let user_repo = app_state.user_repo.clone();

    user_repo
        .follow_repository(user_id, repo_key, body.follow)
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
        .upsert_repository(new_repo)
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

    Ok(Json(AddRepositoryResponse { id }))
}
