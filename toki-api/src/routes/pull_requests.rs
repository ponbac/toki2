use std::cmp;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use az_devops::GitCommitRef;
use serde::Deserialize;
use tracing::instrument;

use crate::{
    app_state::AppStateError,
    auth::AuthSession,
    domain::{PullRequest, RepoKey},
    repositories::UserRepository,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/open", get(open_pull_requests))
        .route("/cached", get(cached_pull_requests))
        .route("/most-recent-commits", get(most_recent_commits))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPullRequestsQuery {
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
async fn open_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<OpenPullRequestsQuery>,
) -> Result<Json<Vec<az_devops::PullRequest>>, AppStateError> {
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
        .collect::<Vec<az_devops::PullRequest>>();
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
#[instrument(name = "GET /cached-pull-requests", skip(auth_session, app_state))]
async fn cached_pull_requests(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;
    let user_repo = app_state.user_repo.clone();
    let followed_repos = user_repo
        .followed_repositories(user_id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut followed_prs = vec![];
    for repo in followed_repos {
        let cached_prs = match app_state.get_cached_pull_requests(repo.clone()).await {
            Ok(prs) => prs,
            Err(_) => {
                tracing::debug!("Error fetching cached PRs for repo: {}", repo);
                continue;
            }
        };

        if let Some(prs) = cached_prs {
            followed_prs.extend(prs);
        } else {
            tracing::debug!("No cached PRs found for repo: {}", repo);
        }
    }
    followed_prs.sort_by_key(|pr| cmp::Reverse(pr.pull_request_base.created_at));

    Ok(Json(followed_prs))
}

#[instrument(name = "GET /most-recent-commits", skip(app_state))]
async fn most_recent_commits(
    State(app_state): State<AppState>,
    Query(query): Query<RepoKey>,
) -> Result<Json<Vec<GitCommitRef>>, (StatusCode, String)> {
    let cached_prs = app_state
        .get_cached_pull_requests(query.clone())
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .map(|mut prs| {
            prs.sort_by_key(|pr| pr.pull_request_base.created_at);
            prs
        });

    let mut commits = vec![];
    if let Some(prs) = cached_prs {
        for pr in prs {
            commits.extend(pr.commits);
        }
    }
    commits.sort_by_key(|commit| cmp::Reverse(commit.author.as_ref().unwrap().date));

    Ok(Json(commits))
}
