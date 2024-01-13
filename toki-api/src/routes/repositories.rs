use axum::{extract::State, http::StatusCode, Json};
use az_devops::RepoClient;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::instrument;

use crate::{domain::RepoKey, AppState};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDto {
    id: i32,
    organization: String,
    project: String,
    repo_name: String,
}

#[instrument(name = "GET /repositories", skip(app_state))]
pub async fn get_repositories(State(app_state): State<AppState>) -> Json<Vec<RepositoryDto>> {
    let repos = query_repository_dtos(&app_state.db_pool)
        .await
        .expect("Failed to query repos");

    Json(repos)
}

async fn query_repository_dtos(
    pool: &PgPool,
) -> Result<Vec<RepositoryDto>, Box<dyn std::error::Error>> {
    let repos = sqlx::query_as!(
        RepositoryDto,
        r#"
        SELECT id, organization, project, repo_name
        FROM repositories
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(repos)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRepositoryBody {
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
pub struct AddRepositoryResponse {
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
pub async fn add_repository(
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

    let key = RepoKey::from(&body);
    app_state.insert_repo_client(key.clone(), repo_client).await;
    tracing::info!("Added new repository: {}", key);

    Ok(Json(AddRepositoryResponse { id }))
}

async fn insert_repository(
    pool: &PgPool,
    organization: &str,
    project: &str,
    repo_name: &str,
    token: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let repo_id = sqlx::query!(
        r#"
        INSERT INTO repositories (organization, project, repo_name, token)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        organization,
        project,
        repo_name,
        token
    )
    .fetch_one(pool)
    .await?
    .id;
    tracing::info!(
        "Added repository to DB: {}/{}/{}",
        organization,
        project,
        repo_name
    );

    Ok(repo_id)
}
