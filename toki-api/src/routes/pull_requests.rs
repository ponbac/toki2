use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use az_devops::PullRequest;
use serde::Deserialize;
use tracing::instrument;

use crate::{app_state::AppStateError, domain::RepoKey, AppState};

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedPullRequestsQuery {
    organization: String,
    project: String,
    repo_name: String,
}

impl From<&ChangedPullRequestsQuery> for RepoKey {
    fn from(query: &ChangedPullRequestsQuery) -> Self {
        Self::new(&query.organization, &query.project, &query.repo_name)
    }
}

// TODO: Global error type!
// #[instrument(name = "GET /changed-pull-requests", skip(app_state))]
// pub async fn changed_pull_requests(
//     State(app_state): State<AppState>,
//     Query(query): Query<ChangedPullRequestsQuery>,
// ) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
//     let changed_pull_requests = app_state
//         .with_differ(&query, |differ| {
//             differ.prev_pull_requests.clone().unwrap_or_default()
//         })
//         .await
//         .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

//     tracing::debug!(
//         "Found {} changed pull requests: [{}]",
//         changed_pull_requests.len(),
//         changed_pull_requests
//             .iter()
//             .map(|pr| pr.title.clone())
//             .collect::<Vec<String>>()
//             .join(", ")
//     );

//     Ok(Json(changed_pull_requests))
// }
