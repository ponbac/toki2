use std::collections::{HashMap, HashSet};

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{
        BoardResponse, FormatForLlmResponse, IterationResponse, PullRequestApprovalStatusResponse,
        PullRequestRefResponse, PullRequestReviewerResponse, WorkItemProjectResponse,
        WorkItemResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::{
        models::{BoardData, PullRequestRef, WorkItem},
        Email, RepoKey, WorkItemError,
    },
};

use super::ApiError;

// ---------------------------------------------------------------------------
// Query parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectQuery {
    pub organization: String,
    pub project: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardQuery {
    pub organization: String,
    pub project: String,
    pub iteration_path: Option<String>,
    pub team: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatForLlmQuery {
    pub organization: String,
    pub project: String,
    pub work_item_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemImageQuery {
    pub organization: String,
    pub project: String,
    pub image_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveWorkItemBody {
    pub organization: String,
    pub project: String,
    pub work_item_id: String,
    pub target_column_name: String,
    pub iteration_path: Option<String>,
    pub team: Option<String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct PullRequestApprovalIndexKey {
    work_item_id: String,
    pull_request_id: String,
    repository_id: String,
}

impl PullRequestApprovalIndexKey {
    fn new(work_item_id: &str, pull_request_id: &str, repository_id: &str) -> Self {
        Self {
            work_item_id: work_item_id.to_string(),
            pull_request_id: pull_request_id.to_string(),
            repository_id: repository_id.to_ascii_lowercase(),
        }
    }
}

#[derive(Debug, Clone)]
struct PullRequestRefEnrichment {
    title: String,
    source_branch: String,
    approval_status: PullRequestApprovalStatusResponse,
}

const DEFAULT_WORK_ITEM_IMAGE_MIME: &str = "application/octet-stream";
const WORK_ITEM_IMAGE_CACHE_CONTROL: &str = "private, max-age=3600";

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

#[instrument(name = "GET /work-items/projects")]
async fn get_projects(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<WorkItemProjectResponse>>, ApiError> {
    let projects = app_state
        .work_item_factory
        .get_available_projects(user.id)
        .await?;
    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

#[instrument(name = "GET /work-items/iterations")]
async fn get_iterations(
    State(app_state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<IterationResponse>>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let iterations = service.get_iterations().await?;
    Ok(Json(iterations.into_iter().map(Into::into).collect()))
}

#[instrument(name = "GET /work-items/board")]
async fn get_board(
    State(app_state): State<AppState>,
    Query(query): Query<BoardQuery>,
) -> Result<Json<BoardResponse>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let mut board_data = service
        .get_board_data(query.iteration_path.as_deref(), query.team.as_deref())
        .await?;
    apply_avatar_overrides_to_work_items(&app_state, &mut board_data.items).await?;
    let approval_index =
        build_pull_request_approval_index(&app_state, &query, &board_data.items).await?;
    let response = board_response_from_enriched_board(board_data, &approval_index);

    Ok(Json(response))
}

#[instrument(name = "GET /work-items/format-for-llm")]
async fn format_for_llm(
    State(app_state): State<AppState>,
    Query(query): Query<FormatForLlmQuery>,
) -> Result<Json<FormatForLlmResponse>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let (markdown, has_images) = service
        .format_work_item_for_llm(&query.work_item_id)
        .await?;
    Ok(Json(FormatForLlmResponse {
        markdown,
        has_images,
    }))
}

#[instrument(name = "GET /work-items/image")]
async fn get_image(
    State(app_state): State<AppState>,
    Query(query): Query<WorkItemImageQuery>,
) -> Result<Response, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let image = service
        .fetch_image(&query.image_url)
        .await
        .map_err(map_work_item_image_error)?;

    let mut response = Response::new(Body::from(image.bytes));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(
            image
                .content_type
                .as_deref()
                .unwrap_or(DEFAULT_WORK_ITEM_IMAGE_MIME),
        )
        .unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_WORK_ITEM_IMAGE_MIME)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(WORK_ITEM_IMAGE_CACHE_CONTROL),
    );

    Ok(response)
}

#[instrument(
    name = "POST /work-items/move",
    fields(
        organization = %body.organization,
        project = %body.project,
        work_item_id = %body.work_item_id,
        target_column_name = %body.target_column_name
    )
)]
async fn move_work_item(
    State(app_state): State<AppState>,
    Json(body): Json<MoveWorkItemBody>,
) -> Result<StatusCode, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&body.organization, &body.project)
        .await?;

    service
        .move_work_item_to_column(
            &body.work_item_id,
            &body.target_column_name,
            body.iteration_path.as_deref(),
            body.team.as_deref(),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn build_pull_request_approval_index(
    app_state: &AppState,
    query: &BoardQuery,
    board_items: &[WorkItem],
) -> Result<HashMap<PullRequestApprovalIndexKey, PullRequestRefEnrichment>, ApiError> {
    let referenced_repository_ids = board_items
        .iter()
        .flat_map(|item| {
            item.pull_requests
                .iter()
                .map(|pr| pr.repository_id.to_ascii_lowercase())
        })
        .collect::<HashSet<_>>();
    if referenced_repository_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let referenced_pr_ids = board_items
        .iter()
        .flat_map(|item| item.pull_requests.iter().map(|pr| pr.id.clone()))
        .collect::<HashSet<_>>();
    let referenced_work_item_ids = board_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();

    let mut enrichment_by_pr_ref = HashMap::new();
    let board_scope_repos = app_state.get_repo_keys().await;

    for repo_key in board_scope_repos {
        if !repo_matches_board_scope(&repo_key, query) {
            continue;
        }

        let repo_client = match app_state.get_repo_client(repo_key.clone()).await {
            Ok(client) => client,
            Err(error) => {
                tracing::debug!(
                    error = %error,
                    repo_key = %repo_key,
                    "Skipping board PR approval enrichment for repo without active client"
                );
                continue;
            }
        };
        let repository_id = repo_client.repo_id().to_ascii_lowercase();
        if !referenced_repository_ids.contains(&repository_id) {
            continue;
        }

        let cached_pull_requests = match app_state.get_cached_pull_requests(repo_key.clone()).await
        {
            Ok(Some(prs)) => prs,
            Ok(None) => continue,
            Err(error) => {
                tracing::debug!(
                    error = %error,
                    repo_key = %repo_key,
                    "Skipping board PR approval enrichment for repo without cache"
                );
                continue;
            }
        };

        for pull_request in cached_pull_requests {
            let pull_request_id = pull_request.pull_request_base.id.to_string();
            if !referenced_pr_ids.contains(&pull_request_id) {
                continue;
            }

            let blocked_by = pull_request.blocked_by(&pull_request.threads);
            let approved_by = pull_request.approved_by();

            let enrichment = PullRequestRefEnrichment {
                title: pull_request.pull_request_base.title.clone(),
                source_branch: pull_request.pull_request_base.source_branch.clone(),
                approval_status: PullRequestApprovalStatusResponse {
                    approved_by: approved_by
                        .into_iter()
                        .map(|reviewer| to_pull_request_reviewer_response(reviewer.identity))
                        .collect(),
                    blocked_by: blocked_by
                        .into_iter()
                        .map(|reviewer| to_pull_request_reviewer_response(reviewer.identity))
                        .collect(),
                },
            };

            for work_item in &pull_request.work_items {
                let work_item_id = work_item.id.to_string();
                if !referenced_work_item_ids.contains(&work_item_id) {
                    continue;
                }
                let key = PullRequestApprovalIndexKey::new(
                    &work_item_id,
                    &pull_request_id,
                    &repository_id,
                );
                enrichment_by_pr_ref
                    .entry(key)
                    .or_insert_with(|| enrichment.clone());
            }
        }
    }

    Ok(enrichment_by_pr_ref)
}

fn repo_matches_board_scope(repo_key: &RepoKey, query: &BoardQuery) -> bool {
    repo_key
        .organization
        .eq_ignore_ascii_case(&query.organization)
        && repo_key.project.eq_ignore_ascii_case(&query.project)
}

fn to_pull_request_reviewer_response(identity: az_devops::Identity) -> PullRequestReviewerResponse {
    PullRequestReviewerResponse {
        id: identity.id,
        display_name: identity.display_name,
        unique_name: identity.unique_name,
        avatar_url: identity.avatar_url,
    }
}

fn board_response_from_enriched_board(
    board_data: BoardData,
    approval_index: &HashMap<PullRequestApprovalIndexKey, PullRequestRefEnrichment>,
) -> BoardResponse {
    BoardResponse {
        columns: board_data.columns.into_iter().map(Into::into).collect(),
        items: board_data
            .items
            .into_iter()
            .map(|item| {
                let item_id = item.id.clone();
                let pull_requests = item.pull_requests.clone();
                let mut item_response: WorkItemResponse = item.into();
                item_response.pull_requests = pull_requests
                    .into_iter()
                    .map(|pr_ref| enrich_pull_request_ref(&item_id, pr_ref, approval_index))
                    .collect();
                item_response
            })
            .collect(),
    }
}

fn enrich_pull_request_ref(
    work_item_id: &str,
    pr_ref: PullRequestRef,
    approval_index: &HashMap<PullRequestApprovalIndexKey, PullRequestRefEnrichment>,
) -> PullRequestRefResponse {
    let key = PullRequestApprovalIndexKey::new(work_item_id, &pr_ref.id, &pr_ref.repository_id);
    let mut pr_response: PullRequestRefResponse = pr_ref.into();

    if let Some(enriched) = approval_index.get(&key) {
        pr_response.title = Some(enriched.title.clone());
        pr_response.source_branch = Some(enriched.source_branch.clone());
        pr_response.approval_status = Some(enriched.approval_status.clone());
    }

    pr_response
}

fn map_work_item_image_error(error: WorkItemError) -> ApiError {
    match error {
        WorkItemError::InvalidInput(message) => {
            if message.to_ascii_lowercase().contains("exceeds") {
                ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, message)
            } else {
                ApiError::bad_request(message)
            }
        }
        WorkItemError::ProviderError(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("not found") || lower.contains("404") {
                ApiError::not_found("work item image not found")
            } else if lower.contains("forbidden") || lower.contains("403") {
                ApiError::forbidden("access to work item image was denied")
            } else {
                ApiError::internal(message)
            }
        }
    }
}

async fn apply_avatar_overrides_to_work_items(
    app_state: &AppState,
    items: &mut [WorkItem],
) -> Result<(), ApiError> {
    if items.is_empty() {
        return Ok(());
    }

    let mut unique_emails = HashSet::new();
    for item in items.iter() {
        collect_work_item_person_email(&mut unique_emails, item.assigned_to.as_ref());
        collect_work_item_person_email(&mut unique_emails, item.created_by.as_ref());
    }

    if unique_emails.is_empty() {
        return Ok(());
    }

    let email_list = unique_emails.into_iter().collect::<Vec<_>>();
    let avatar_by_email = app_state
        .avatar_service
        .resolve_overrides(&email_list)
        .await?
        .into_iter()
        .map(|override_item| (override_item.email.to_lowercase(), override_item.avatar_url))
        .collect::<HashMap<_, _>>();

    for item in items.iter_mut() {
        apply_avatar_override_to_work_item_person(item.assigned_to.as_mut(), &avatar_by_email);
        apply_avatar_override_to_work_item_person(item.created_by.as_mut(), &avatar_by_email);
    }

    Ok(())
}

fn collect_work_item_person_email(
    emails: &mut HashSet<String>,
    person: Option<&crate::domain::models::WorkItemPerson>,
) {
    let Some(person) = person else {
        return;
    };

    let Some(unique_name) = person.unique_name.as_deref() else {
        return;
    };

    if let Some(email) = Email::normalize_lookup_key(unique_name) {
        emails.insert(email);
    }
}

fn apply_avatar_override_to_work_item_person(
    person: Option<&mut crate::domain::models::WorkItemPerson>,
    avatar_by_email: &HashMap<String, String>,
) {
    let Some(person) = person else {
        return;
    };

    let Some(unique_name) = person.unique_name.as_deref() else {
        return;
    };

    let Some(email) = Email::normalize_lookup_key(unique_name) else {
        return;
    };

    if let Some(avatar_url) = avatar_by_email.get(&email) {
        person.image_url = Some(avatar_url.clone());
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(get_projects))
        .route("/iterations", get(get_iterations))
        .route("/board", get(get_board))
        .route("/image", get(get_image))
        .route("/format-for-llm", get(format_for_llm))
        .route("/move", post(move_work_item))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use time::OffsetDateTime;

    use crate::{
        adapters::inbound::http::PullRequestApprovalStatusResponse,
        domain::{
            models::{
                BoardData, BoardState, PullRequestRef, WorkItem, WorkItemCategory, WorkItemPerson,
            },
            Email,
        },
    };

    use super::{
        apply_avatar_override_to_work_item_person, board_response_from_enriched_board,
        PullRequestApprovalIndexKey, PullRequestRefEnrichment, PullRequestReviewerResponse,
    };

    #[test]
    fn normalize_lookup_key_trims_and_lowercases() {
        assert_eq!(
            Email::normalize_lookup_key("  USER@Example.com "),
            Some("user@example.com".to_string())
        );
        assert_eq!(Email::normalize_lookup_key(""), None);
    }

    #[test]
    fn normalize_lookup_key_falls_back_for_non_email_identity_values() {
        assert_eq!(
            Email::normalize_lookup_key("  Display Name  "),
            Some("display name".to_string())
        );
    }

    #[test]
    fn apply_avatar_override_to_work_item_person_updates_image_url() {
        let mut person = WorkItemPerson {
            display_name: "Test User".to_string(),
            unique_name: Some("USER@example.com".to_string()),
            image_url: Some("https://provider.example.com/avatar.png".to_string()),
        };

        let mut avatar_by_email = HashMap::new();
        avatar_by_email.insert(
            "user@example.com".to_string(),
            "https://custom.example.com/avatar.png".to_string(),
        );

        apply_avatar_override_to_work_item_person(Some(&mut person), &avatar_by_email);

        assert_eq!(
            person.image_url.as_deref(),
            Some("https://custom.example.com/avatar.png")
        );
    }

    #[test]
    fn apply_avatar_override_to_work_item_person_supports_non_email_unique_name_fallback() {
        let mut person = WorkItemPerson {
            display_name: "Display Name".to_string(),
            unique_name: Some("  Display Name  ".to_string()),
            image_url: None,
        };

        let mut avatar_by_email = HashMap::new();
        avatar_by_email.insert(
            "display name".to_string(),
            "https://custom.example.com/avatar.png".to_string(),
        );

        apply_avatar_override_to_work_item_person(Some(&mut person), &avatar_by_email);

        assert_eq!(
            person.image_url.as_deref(),
            Some("https://custom.example.com/avatar.png")
        );
    }

    #[test]
    fn pull_request_approval_index_key_normalizes_repository_id() {
        let key = PullRequestApprovalIndexKey::new("123", "42", "REPO-GUID-ABC");
        assert_eq!(key.repository_id, "repo-guid-abc");
    }

    #[test]
    fn board_response_from_enriched_board_applies_pr_enrichment() {
        let board = BoardData {
            columns: vec![],
            items: vec![sample_work_item()],
        };

        let mut approval_index = HashMap::new();
        approval_index.insert(
            PullRequestApprovalIndexKey::new("123", "42", "repo-guid"),
            PullRequestRefEnrichment {
                title: "Improve board drag behavior".to_string(),
                source_branch: "refs/heads/123/improve-board-drag-behavior".to_string(),
                approval_status: PullRequestApprovalStatusResponse {
                    approved_by: vec![PullRequestReviewerResponse {
                        id: "reviewer-1".to_string(),
                        display_name: "Reviewer One".to_string(),
                        unique_name: "reviewer.one@example.com".to_string(),
                        avatar_url: Some("https://avatars.example.com/1.png".to_string()),
                    }],
                    blocked_by: vec![],
                },
            },
        );

        let response = board_response_from_enriched_board(board, &approval_index);
        let pull_request = &response.items[0].pull_requests[0];

        assert_eq!(
            pull_request.title.as_deref(),
            Some("Improve board drag behavior")
        );
        assert_eq!(
            pull_request.source_branch.as_deref(),
            Some("refs/heads/123/improve-board-drag-behavior")
        );
        let approval_status = pull_request
            .approval_status
            .as_ref()
            .expect("approval status");
        assert_eq!(approval_status.approved_by.len(), 1);
        assert!(approval_status.blocked_by.is_empty());
    }

    #[test]
    fn board_response_from_enriched_board_leaves_unmatched_pr_unenriched() {
        let board = BoardData {
            columns: vec![],
            items: vec![sample_work_item()],
        };

        let response = board_response_from_enriched_board(board, &HashMap::new());
        let pull_request = &response.items[0].pull_requests[0];

        assert!(pull_request.title.is_none());
        assert!(pull_request.source_branch.is_none());
        assert!(pull_request.approval_status.is_none());
    }

    fn sample_work_item() -> WorkItem {
        WorkItem {
            id: "123".to_string(),
            title: "Sample work item".to_string(),
            board_state: BoardState::Todo,
            board_column_id: None,
            board_column_name: None,
            category: WorkItemCategory::Task,
            state_name: "New".to_string(),
            priority: Some(1),
            assigned_to: None,
            created_by: None,
            description: None,
            description_rendered_html: None,
            acceptance_criteria: None,
            iteration_path: None,
            area_path: None,
            tags: vec![],
            parent: None,
            related: vec![],
            pull_requests: vec![PullRequestRef {
                id: "42".to_string(),
                repository_id: "REPO-GUID".to_string(),
                project_id: "project-guid".to_string(),
                url: "https://dev.azure.com/org/project/_git/repo/pullrequest/42".to_string(),
            }],
            url: "https://dev.azure.com/org/project/_workitems/edit/123".to_string(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            changed_at: OffsetDateTime::UNIX_EPOCH,
        }
    }
}
