use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use az_devops::PullRequest;
use serde::Deserialize;
use tracing::instrument;

use crate::{
    app_state::AppStateError,
    domain::{RepoDifferMessage, RepoKey},
    AppState,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPullRequestsQuery {
    organization: String,
    project: String,
    repo_name: String,
    author: Option<String>,
}

impl From<&OpenPullRequestsQuery> for RepoKey {
    fn from(query: &OpenPullRequestsQuery) -> Self {
        Self::new(&query.organization, &query.project, &query.repo_name)
    }
}

#[instrument(name = "GET /pull-requests", skip(app_state))]
pub async fn open_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<OpenPullRequestsQuery>,
) -> Result<Json<Vec<PullRequest>>, AppStateError> {
    let client = app_state.get_repo_client(&query).await?;

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

// TODO: Global error type!
#[instrument(name = "GET /cached-pull-requests", skip(app_state))]
pub async fn cached_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<RepoKey>,
) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
    let cached_prs = app_state
        .get_cached_pull_requests(query)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(cached_prs.unwrap_or_default()))
}

#[instrument(name = "POST /start-differ", skip(app_state))]
pub async fn start_differ(
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

#[instrument(name = "POST /force-update", skip(app_state))]
pub async fn force_update(
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

#[instrument(name = "POST /stop-differ", skip(app_state))]
pub async fn stop_differ(
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
