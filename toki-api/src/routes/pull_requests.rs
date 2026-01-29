use std::cmp;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use az_devops::GitCommitRef;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tracing::instrument;

use crate::{
    app_state::AppStateError,
    auth::AuthSession,
    domain::{PullRequest, RepoKey},
    repositories::UserRepository,
    AppState,
};

use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/open", get(open_pull_requests))
        .route("/cached", get(cached_pull_requests))
        .route("/list", get(list_pull_requests))
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

#[instrument(name = "GET /cached-pull-requests", skip(auth_session, app_state))]
async fn cached_pull_requests(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PullRequest>>, ApiError> {
    let followed_prs = get_followed_pull_requests(&auth_session, app_state).await?;
    Ok(Json(followed_prs))
}

#[instrument(name = "GET /most-recent-commits", skip(app_state))]
async fn most_recent_commits(
    State(app_state): State<AppState>,
    Query(query): Query<RepoKey>,
) -> Result<Json<Vec<GitCommitRef>>, ApiError> {
    let cached_prs = app_state
        .get_cached_pull_requests(query.clone())
        .await?
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

/// A trimmed down version of a pull request, only containing the fields we need for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListPullRequest {
    organization: String,
    project: String,
    repo_name: String,
    id: i32,
    title: String,
    created_by: az_devops::Identity,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    source_branch: String,
    target_branch: String,
    is_draft: bool,
    merge_status: Option<az_devops::MergeStatus>,
    threads: Vec<az_devops::Thread>,
    work_items: Vec<az_devops::WorkItem>,
    reviewers: Vec<az_devops::IdentityWithVote>,
    blocked_by: Vec<az_devops::IdentityWithVote>,
    approved_by: Vec<az_devops::IdentityWithVote>,
    waiting_for_user_review: bool,
    review_required: bool,
}

impl ListPullRequest {
    fn from_pull_request(pr: PullRequest, user_email: &str) -> Self {
        let blocked_by = pr.blocked_by(&pr.threads);
        let approved_by = pr.approved_by();
        let (waiting_for_user_review, review_required) = pr.waiting_for_user_review(user_email);
        Self {
            organization: pr.organization,
            project: pr.project,
            repo_name: pr.repo_name,
            id: pr.pull_request_base.id,
            title: pr.pull_request_base.title,
            created_by: pr.pull_request_base.created_by,
            created_at: pr.pull_request_base.created_at,
            source_branch: pr.pull_request_base.source_branch,
            target_branch: pr.pull_request_base.target_branch,
            is_draft: pr.pull_request_base.is_draft,
            merge_status: pr.pull_request_base.merge_status,
            threads: pr.threads,
            work_items: pr.work_items,
            reviewers: pr.pull_request_base.reviewers,
            blocked_by,
            approved_by,
            waiting_for_user_review,
            review_required,
        }
    }
}

#[instrument(name = "GET /pull-requests/list", skip(auth_session, app_state))]
async fn list_pull_requests(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<ListPullRequest>>, ApiError> {
    let mut followed_prs = get_followed_pull_requests(&auth_session, app_state).await?;
    followed_prs.sort_by_key(|pr| cmp::Reverse(pr.pull_request_base.created_at));

    Ok(Json(
        followed_prs
            .into_iter()
            .map(|pr| {
                ListPullRequest::from_pull_request(pr, &auth_session.user.as_ref().unwrap().email)
            })
            .collect(),
    ))
}

/// Get the followed pull requests from the cache.
///
/// This function will fetch the cached pull requests from the cache and replace the mentions in the threads with names instead of ids.
async fn get_followed_pull_requests(
    auth_session: &AuthSession,
    app_state: AppState,
) -> Result<Vec<PullRequest>, ApiError> {
    let user_id = auth_session.user.as_ref().expect("user not found").id;
    let user_repo = app_state.user_repo.clone();
    let followed_repos = user_repo.followed_repositories(&user_id).await?;

    let mut followed_prs = vec![];
    for repo_key in &followed_repos {
        match app_state.get_cached_pull_requests(repo_key.clone()).await {
            Ok(Some(prs)) => {
                let identities = app_state
                    .get_cached_identities(repo_key.clone())
                    .await?;
                followed_prs.extend(
                    prs.iter()
                        .map(|pr| pr.with_replaced_mentions(&identities.id_to_name_map())),
                );
            }
            Ok(None) => {
                tracing::debug!("No cached PRs found for repo: {}", repo_key);
            }
            Err(_) => {
                tracing::debug!("Error fetching cached PRs for repo: {}", repo_key);
                continue;
            }
        };
    }

    Ok(followed_prs)
}
