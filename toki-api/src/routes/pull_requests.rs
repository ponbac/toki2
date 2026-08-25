use std::{
    cmp,
    collections::{HashMap, HashSet},
};

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    adapters::inbound::http::ListPullRequestResponse,
    app_state::AppStateError,
    auth::AuthUser,
    domain::{
        Email, PullRequest, PullRequestCommit, PullRequestIdentity, PullRequestReviewer, RepoKey,
    },
    observability::{record_repo_key, record_user_id},
    repositories::UserRepository,
    AppState,
};

use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_pull_requests))
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

async fn open_pull_requests(
    State(app_state): State<AppState>,
    Query(query): Query<OpenPullRequestsQuery>,
) -> Result<Json<Vec<az_devops::PullRequest>>, AppStateError> {
    record_repo_key(RepoKey::from(&query));
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

async fn cached_pull_requests(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<PullRequest>>, ApiError> {
    record_user_id(user.id);
    let mut followed_prs = get_followed_pull_requests(&app_state, &user).await?;
    apply_avatar_overrides_to_pull_requests(&app_state, &mut followed_prs).await?;
    Ok(Json(followed_prs))
}

async fn most_recent_commits(
    State(app_state): State<AppState>,
    Query(query): Query<RepoKey>,
) -> Result<Json<Vec<PullRequestCommit>>, ApiError> {
    record_repo_key(&query);
    let cached_prs = app_state
        .get_cached_pull_requests(query.clone())
        .await?
        .map(|mut prs| {
            prs.sort_by_key(|pr| pr.created_at);
            prs
        });

    let mut commits = vec![];
    if let Some(prs) = cached_prs {
        for pr in prs {
            commits.extend(pr.commits);
        }
    }
    commits.sort_by_key(|commit| cmp::Reverse(commit.author.date));

    Ok(Json(commits))
}

/// List followed pull requests
///
/// Returns a Toki-owned projection of pull requests the user follows, including
/// identities, merge status, simplified threads, and linked work-item refs.
#[utoipa::path(
    get,
    path = "/pull-requests/list",
    operation_id = "listPullRequests",
    tag = "Pull requests",
    responses(
        (status = 200, description = "Followed pull requests", body = Vec<ListPullRequestResponse>),
        (status = 401, description = "Missing or invalid credentials")
    )
)]
pub async fn list_pull_requests(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<ListPullRequestResponse>>, ApiError> {
    record_user_id(user.id);
    let mut followed_prs = get_followed_pull_requests(&app_state, &user).await?;
    apply_avatar_overrides_to_pull_requests(&app_state, &mut followed_prs).await?;
    followed_prs.sort_by_key(|pr| cmp::Reverse(pr.created_at));

    let list_prs = followed_prs
        .into_iter()
        .map(|pr| ListPullRequestResponse::from_domain(pr, &user.email))
        .collect::<Vec<_>>();

    Ok(Json(list_prs))
}

/// Get the followed pull requests from the cache.
///
/// This function will fetch the cached pull requests from the cache and replace the mentions in the threads with names instead of ids.
async fn get_followed_pull_requests(
    app_state: &AppState,
    user: &AuthUser,
) -> Result<Vec<PullRequest>, ApiError> {
    let user_repo = app_state.user_repo.clone();
    let followed_repos = user_repo.followed_repositories(user.id).await?;

    let mut followed_prs = vec![];
    for repo_key in &followed_repos {
        match app_state.get_cached_pull_requests(repo_key.clone()).await {
            Ok(Some(prs)) => {
                let identities = app_state.get_cached_identities(repo_key.clone()).await?;
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

async fn apply_avatar_overrides_to_pull_requests(
    app_state: &AppState,
    prs: &mut [PullRequest],
) -> Result<(), ApiError> {
    if prs.is_empty() {
        return Ok(());
    }

    let mut unique_emails = HashSet::new();

    for pr in prs.iter() {
        unique_emails.extend(collect_pr_participant_emails(pr));
    }

    if unique_emails.is_empty() {
        return Ok(());
    }

    let email_list = unique_emails.into_iter().collect::<Vec<_>>();
    let overrides = app_state
        .avatar_service
        .resolve_overrides(&email_list)
        .await?;

    let avatar_by_email = overrides
        .into_iter()
        .map(|item| (item.email.to_lowercase(), item.avatar_url))
        .collect::<HashMap<_, _>>();

    for pr in prs.iter_mut() {
        apply_avatar_overrides_to_pull_request(pr, &avatar_by_email);
    }

    Ok(())
}

fn collect_pr_participant_emails(pr: &PullRequest) -> HashSet<String> {
    let mut emails = HashSet::new();
    collect_identity_email(&mut emails, &pr.created_by);
    collect_reviewer_emails(&mut emails, &pr.reviewers);

    for thread in &pr.threads {
        for comment in &thread.comments {
            collect_identity_email(&mut emails, &comment.author);
            for liker in &comment.liked_by {
                collect_identity_email(&mut emails, liker);
            }
        }
    }

    emails
}

fn collect_reviewer_emails(emails: &mut HashSet<String>, identities: &[PullRequestReviewer]) {
    for reviewer in identities {
        collect_identity_email(emails, &reviewer.identity);
    }
}

fn collect_identity_email(emails: &mut HashSet<String>, identity: &PullRequestIdentity) {
    if let Some(email) = Email::normalize_lookup_key(&identity.unique_name) {
        emails.insert(email);
    }
}

fn apply_avatar_overrides_to_pull_request(
    pr: &mut PullRequest,
    avatar_by_email: &HashMap<String, String>,
) {
    apply_avatar_override_to_identity(&mut pr.created_by, avatar_by_email);

    for reviewer in &mut pr.reviewers {
        apply_avatar_override_to_identity(&mut reviewer.identity, avatar_by_email);
    }

    for thread in &mut pr.threads {
        for comment in &mut thread.comments {
            apply_avatar_override_to_identity(&mut comment.author, avatar_by_email);
            for liker in &mut comment.liked_by {
                apply_avatar_override_to_identity(liker, avatar_by_email);
            }
        }
    }
}

fn apply_avatar_override_to_identity(
    identity: &mut PullRequestIdentity,
    avatar_by_email: &HashMap<String, String>,
) {
    let Some(email) = Email::normalize_lookup_key(&identity.unique_name) else {
        return;
    };

    if let Some(avatar_url) = avatar_by_email.get(&email) {
        identity.avatar_url = Some(avatar_url.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::{Email, PullRequestIdentity};

    use super::apply_avatar_override_to_identity;

    #[test]
    fn normalize_lookup_key_trims_and_lowercases() {
        assert_eq!(
            Email::normalize_lookup_key("  USER@Example.com  "),
            Some("user@example.com".to_string())
        );
        assert_eq!(Email::normalize_lookup_key("   "), None);
    }

    #[test]
    fn normalize_lookup_key_falls_back_for_non_email_identity_values() {
        assert_eq!(
            Email::normalize_lookup_key("  Display Name  "),
            Some("display name".to_string())
        );
    }

    #[test]
    fn apply_avatar_override_to_identity_replaces_avatar_url() {
        let mut identity = PullRequestIdentity {
            id: "user-id".to_string(),
            display_name: "Test User".to_string(),
            unique_name: "USER@example.com".to_string(),
            avatar_url: Some("https://provider.example.com/avatar.png".to_string()),
        };

        let mut avatar_by_email = HashMap::new();
        avatar_by_email.insert(
            "user@example.com".to_string(),
            "https://custom.example.com/avatar.png".to_string(),
        );

        apply_avatar_override_to_identity(&mut identity, &avatar_by_email);

        assert_eq!(
            identity.avatar_url.as_deref(),
            Some("https://custom.example.com/avatar.png")
        );
    }

    #[test]
    fn apply_avatar_override_to_identity_supports_non_email_unique_name_fallback() {
        let mut identity = PullRequestIdentity {
            id: "user-id".to_string(),
            display_name: "Test User".to_string(),
            unique_name: "  Display Name  ".to_string(),
            avatar_url: None,
        };

        let mut avatar_by_email = HashMap::new();
        avatar_by_email.insert(
            "display name".to_string(),
            "https://custom.example.com/avatar.png".to_string(),
        );

        apply_avatar_override_to_identity(&mut identity, &avatar_by_email);

        assert_eq!(
            identity.avatar_url.as_deref(),
            Some("https://custom.example.com/avatar.png")
        );
    }
}
