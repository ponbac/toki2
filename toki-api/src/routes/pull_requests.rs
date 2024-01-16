use axum::{
    extract::{Query, State},
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
