use crate::repositories::RepoRepository;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use time::OffsetDateTime;
use tracing::instrument;

use crate::{
    app_state::AppState,
    auth::AuthSession,
    domain::{RepoDifferMessage, RepoDifferStatus, RepoKey},
    repositories::UserRepository,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_differs))
        .route("/start", post(start_differ))
        .route("/stop", post(stop_differ))
        .route("/force", post(force_update))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Differ {
    #[serde(flatten)]
    key: RepoKey,
    status: RepoDifferStatus,
    #[serde(with = "time::serde::rfc3339::option")]
    last_updated: Option<OffsetDateTime>,
    refresh_interval: Option<Duration>,
    followed: bool,
    is_invalid: bool,
}

#[instrument(name = "get_differs", skip(user, app_state))]
async fn get_differs(
    AuthSession { user, .. }: AuthSession,
    State(app_state): State<AppState>,
) -> Json<Vec<Differ>> {
    let user_repo = app_state.user_repo.clone();
    let followed_repos = user_repo
        .followed_repositories(user.expect("user should not be None").id)
        .await
        .expect("Failed to query followed repos");

    let differs = app_state.get_repo_differs().await;
    let mut differ_dtos = Vec::new();
    for differ in differs {
        let differ = differ.clone();

        let key = differ.key.clone();
        let status = *differ.status.read().await;
        let last_updated = *differ.last_updated.read().await;
        let refresh_interval = *differ.interval.read().await;

        differ_dtos.push(Differ {
            key: key.clone(),
            status,
            last_updated,
            refresh_interval,
            followed: followed_repos.contains(&key),
            is_invalid: false,
        });
    }

    let repositories_repo = app_state.repository_repo.clone();
    let all_repos = repositories_repo
        .get_repositories()
        .await
        .expect("Failed to query all repos");
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
            });
        }
    }

    Json(differ_dtos)
}

#[instrument(name = "start_differ", skip(app_state))]
async fn start_differ(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sender = app_state
        .get_differ_sender(body)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let _ = sender
        .send(RepoDifferMessage::Start(Duration::from_secs(300)))
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));

    Ok(StatusCode::OK)
}

#[instrument(name = "stop_differ", skip(app_state))]
async fn stop_differ(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sender = app_state
        .get_differ_sender(body)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let _ = sender
        .send(RepoDifferMessage::Stop)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));

    Ok(StatusCode::OK)
}

#[instrument(name = "force_update", skip(app_state))]
async fn force_update(
    State(app_state): State<AppState>,
    Json(body): Json<RepoKey>,
) -> Result<StatusCode, (StatusCode, String)> {
    let sender = app_state
        .get_differ_sender(body)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let _ = sender
        .send(RepoDifferMessage::ForceUpdate)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));

    Ok(StatusCode::OK)
}
