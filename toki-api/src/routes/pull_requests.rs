use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use az_devops::{GitCommitRef, PullRequest};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::instrument;

use crate::{app_state::AppStateError, auth::AuthSession, domain::RepoKey, AppState};

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
#[instrument(name = "GET /cached-pull-requests", skip(auth_session, app_state))]
async fn cached_pull_requests(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Query(query): Query<RepoKey>,
) -> Result<Json<Vec<PullRequest>>, (StatusCode, String)> {
    let user_id = auth_session.user.expect("user not found").id;
    let followed_repos = query_followed_by_user(&app_state.db_pool, user_id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut followed_prs = vec![];
    for repo in followed_repos {
        let cached_prs = app_state
            .get_cached_pull_requests(repo.clone())
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        if let Some(prs) = cached_prs {
            followed_prs.extend(prs);
        }
    }
    followed_prs.sort_by_key(|pr| pr.created_at);

    Ok(Json(followed_prs))
}

async fn query_followed_by_user(
    pool: &PgPool,
    user_id: i32,
) -> Result<Vec<RepoKey>, Box<dyn std::error::Error>> {
    sqlx::query_as!(
        RepoKey,
        r#"
        SELECT organization, project, repo_name
        FROM user_repositories
        JOIN repositories ON user_repositories.repository_id = repositories.id
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
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
            prs.sort_by_key(|pr| pr.created_at);
            prs
        });

    let client = app_state.get_repo_client(query).await.map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get repository client: {}", err),
        )
    })?;
    let mut commits = vec![];
    if let Some(prs) = cached_prs {
        for pr in prs {
            let pr_commits = pr.commits(&client).await.map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get commits in pull request: {}", err),
                )
            })?;
            commits.extend(pr_commits);
        }
    }

    commits.sort_by_key(|commit| commit.author.as_ref().unwrap().date);
    commits.reverse();

    Ok(Json(commits))
}
