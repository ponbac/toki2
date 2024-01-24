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
    domain::{RepoDifferMessage, RepoDifferStatus, RepoKey},
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
}

#[instrument(name = "get_differs", skip(app_state))]
async fn get_differs(State(app_state): State<AppState>) -> Json<Vec<Differ>> {
    let differs = app_state.get_repo_differs().await;

    let mut differ_dtos = Vec::new();
    for differ in differs {
        let differ = differ.clone();

        let key = differ.key.clone();
        let status = *differ.status.read().await;
        let last_updated = *differ.last_updated.read().await;
        let refresh_interval = *differ.interval.read().await;

        differ_dtos.push(Differ {
            key,
            status,
            last_updated,
            refresh_interval,
        });
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
        .send(RepoDifferMessage::Start(Duration::from_secs(30)))
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
