use crate::{auth::AuthBackend, domain::Role, repositories::RepoRepository};
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use axum_login::permission_required;
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    app_state::AppState,
    auth::AuthUser,
    domain::{RepoDifferMessage, RepoDifferStatus, RepoKey},
    repositories::UserRepository,
};

use super::ApiError;
use crate::observability::{record_repo_key, record_user_id};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/start", post(start_differ))
        .route("/stop", post(stop_differ))
        .route("/force", post(force_update))
        .route_layer(permission_required!(AuthBackend, Role::Admin))
        .route("/", get(get_differs))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Differ {
    #[serde(flatten)]
    key: RepoKey,
    repo_id: i32,
    status: RepoDifferStatus,
    #[serde(with = "time::serde::rfc3339::option")]
    last_updated: Option<OffsetDateTime>,
    refresh_interval: Option<Duration>,
    followed: bool,
    is_invalid: bool,
}

async fn get_differs(user: AuthUser, State(app_state): State<AppState>) -> Json<Vec<Differ>> {
    record_user_id(user.id);
    let user_repo = app_state.user_repo.clone();
    let repositories_repo = app_state.repository_repo.clone();
    let all_repos = repositories_repo
        .get_repositories()
        .await
        .expect("Failed to query all repos");
    let followed_repos = user_repo
        .followed_repositories(user.id)
        .await
        .expect("Failed to query followed repos");

    let differs = app_state.get_repo_differs().await;
    let mut differ_dtos = Vec::new();
    for differ in differs {
        let key = differ.repository().clone();
        let status = *differ.status.read().await;
        let last_updated = *differ.last_updated.read().await;
        let refresh_interval = *differ.interval.read().await;
        let followed = followed_repos.contains(&key);
        let repo_id = all_repos
            .iter()
            .find(|repository| RepoKey::from(*repository) == key)
            .expect("repo differ should correspond to a stored repository")
            .id;

        differ_dtos.push(Differ {
            key,
            status,
            last_updated,
            refresh_interval,
            followed,
            is_invalid: false,
            repo_id,
        });
    }

    // add repos not found in differs, meaning no client is created for them
    for repo in all_repos {
        let key = RepoKey::new(&repo.organization, &repo.project, &repo.repo_name);
        if !differ_dtos.iter().any(|d| d.key == key) {
            differ_dtos.push(Differ {
                key: key.clone(),
                status: RepoDifferStatus::Errored,
                last_updated: None,
                refresh_interval: None,
                followed: followed_repos.contains(&key),
                is_invalid: true,
                repo_id: repo.id,
            });
        }
    }

    Json(differ_dtos)
}

async fn start_differ(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, ApiError> {
    record_repo_key(&body);
    let sender = app_state.get_differ_sender(body).await?;

    let _ = sender
        .send(RepoDifferMessage::Start(Duration::from_secs(300)))
        .await;

    Ok(StatusCode::OK)
}

async fn stop_differ(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, ApiError> {
    record_repo_key(&body);
    let sender = app_state.get_differ_sender(body).await?;

    let _ = sender.send(RepoDifferMessage::Stop).await;

    Ok(StatusCode::OK)
}

async fn force_update(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, ApiError> {
    record_repo_key(&body);
    let sender = app_state.get_differ_sender(body).await?;

    let _ = sender.send(RepoDifferMessage::ForceUpdate).await;

    Ok(StatusCode::OK)
}
