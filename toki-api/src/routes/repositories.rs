use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use az_devops::RepoClient;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::instrument;

use crate::{auth::AuthSession, domain::RepoKey, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_repositories))
        .route("/", post(add_repository))
        .route("/follow", post(follow_repository))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryDto {
    id: i32,
    organization: String,
    project: String,
    repo_name: String,
}

#[instrument(name = "GET /repositories", skip(app_state))]
async fn get_repositories(State(app_state): State<AppState>) -> Json<Vec<RepositoryDto>> {
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

async fn query_repository(
    pool: &PgPool,
    key: RepoKey,
) -> Result<Option<RepositoryDto>, Box<dyn std::error::Error>> {
    let repo = sqlx::query_as!(
        RepositoryDto,
        r#"
        SELECT id, organization, project, repo_name
        FROM repositories
        WHERE organization = $1 AND project = $2 AND repo_name = $3
        "#,
        key.organization,
        key.project,
        key.repo_name
    )
    .fetch_optional(pool)
    .await?;

    Ok(repo)
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
    let repository_id = match query_repository(&app_state.db_pool, repo_key).await {
        Ok(Some(repo)) => repo.id,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                format!(
                    "Repository not found: {}/{}/{}",
                    body.organization, body.project, body.repo_name
                ),
            ))
        }
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query repository: {}", err),
            ))
        }
    };

    insert_follow(&app_state.db_pool, user_id, repository_id, body.follow)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to insert follow: {}", err),
            )
        })?;

    Ok(Json(()))
}

async fn insert_follow(
    pool: &PgPool,
    user_id: i32,
    repository_id: i32,
    follow: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if follow {
        sqlx::query!(
            r#"
            INSERT INTO user_repositories (user_id, repository_id)
            VALUES ($1, $2)
            ON CONFLICT (user_id, repository_id) DO NOTHING
            "#,
            user_id,
            repository_id
        )
        .execute(pool)
        .await?;
    } else {
        sqlx::query!(
            r#"
            DELETE FROM user_repositories
            WHERE user_id = $1 AND repository_id = $2
            "#,
            user_id,
            repository_id
        )
        .execute(pool)
        .await?;
    }

    Ok(())
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

    let key = RepoKey::from(&body);
    app_state.insert_repo(key.clone(), repo_client).await;
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
        ON CONFLICT (project, repo_name, organization) 
        DO UPDATE SET
        token = EXCLUDED.token
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
