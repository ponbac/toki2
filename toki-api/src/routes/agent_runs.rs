use std::{
    path::{Path as FsPath, PathBuf},
    process::Stdio,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::{fs, process::Command};
use url::Url;

use crate::{
    auth::AuthUser,
    domain::{models::WorkItemProject, RepoKey},
    repositories::{
        AdoWorkItemSourceMetadata, AgentRunActorMetadata, AgentRunIssueSummary,
        AgentRunWorkItemRow, UserRepository, AZURE_DEVOPS_WORK_ITEM_SOURCE_PROVIDER,
    },
    AppState,
};

use super::ApiError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_agent_run))
        .route(
            "/latest-by-work-items",
            post(latest_agent_runs_by_work_items),
        )
        .route("/:id", get(get_agent_run).delete(delete_agent_run))
        .route("/:id/events", get(get_agent_run_events))
        .route("/:id/feedback", post(send_agent_run_feedback))
        .route("/:id/approve-plan", post(approve_agent_run_plan))
        .route("/:id/cancel", post(cancel_agent_run))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAgentRunBody {
    source: AdoWorkItemSource,
    target_repo: AgentRunTargetRepo,
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdoWorkItemSource {
    id: String,
    title: String,
    url: String,
    organization: String,
    project: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRunTargetRepo {
    provider: AgentRunTargetRepoProvider,
    organization: String,
    project: String,
    repo_name: String,
    default_branch: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AgentRunTargetRepoProvider {
    AzureDevOps,
}

impl std::fmt::Display for AgentRunTargetRepoProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AzureDevOps => write!(f, "azureDevOps"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRunFeedbackBody {
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunRecord {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) source: AgentRunSource,
    pub(crate) target_repo: AgentRunRecordTargetRepo,
    pub(crate) workpad: AgentRunWorkpad,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub(crate) updated_at: OffsetDateTime,
    #[serde(default)]
    pending_publish: Option<BackendPublishPayload>,
}

impl AgentRunRecord {
    pub(crate) fn draft_pr_url(&self) -> Option<&str> {
        self.workpad.draft_pr_url.as_deref()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunSource {
    pub(crate) id: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunRecordTargetRepo {
    pub(crate) provider: AgentRunTargetRepoProvider,
    clone_url: String,
    pub(crate) default_branch: String,
    pub(crate) organization: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) repo_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunWorkpad {
    #[serde(default)]
    pub(crate) draft_pr_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BackendPublishPayload {
    branch_name: String,
    base_object_id: String,
    title: String,
    description: String,
    changes: Vec<BackendPublishChange>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BackendPublishChange {
    change_type: BackendPublishChangeType,
    path: String,
    content: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
enum BackendPublishChangeType {
    Add,
    Edit,
    Delete,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LatestAgentRunsByWorkItemsRequest {
    source_provider: String,
    organization: String,
    project: String,
    work_item_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatestAgentRunsByWorkItemsResponse {
    runs: Vec<AgentRunIssueSummaryResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentRunIssueSummaryResponse {
    id: String,
    work_item_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft_pr_url: Option<String>,
    created_by: AgentRunIssueSummaryCreatedBy,
    created_at: String,
    updated_at: String,
    last_synced_at: String,
    sync_state: AgentRunSyncState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentRunIssueSummaryCreatedBy {
    display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum AgentRunSyncState {
    Fresh,
    Stale,
}

async fn create_agent_run(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<CreateAgentRunBody>,
) -> Result<Json<Value>, ApiError> {
    ensure_user_has_project_access(
        &app_state,
        &user,
        &body.source.organization,
        &body.source.project,
    )
    .await?;
    ensure_user_follows_repo(&app_state, &user, &body.target_repo).await?;
    let repo_key = target_repo_key(&body.target_repo);
    let git_auth_header = app_state
        .get_repo_client(repo_key)
        .await?
        .git_basic_auth_header();

    let service = app_state
        .work_item_factory
        .create_service(&body.source.organization, &body.source.project)
        .await?;
    let (markdown, _has_images) = service.format_work_item_for_llm(&body.source.id).await?;
    let mut agent_request = json!({
        "mode": "planFirst",
        "source": {
            "type": "adoWorkItem",
            "id": body.source.id,
            "title": body.source.title,
            "url": body.source.url,
            "markdown": markdown,
        },
        "targetRepo": {
            "provider": "azureDevOps",
            "cloneUrl": azure_devops_clone_url(&body.target_repo)?,
            "defaultBranch": body.target_repo.default_branch,
            "organization": body.target_repo.organization,
            "project": body.target_repo.project,
            "repoName": body.target_repo.repo_name,
            "gitAuthHeader": git_auth_header,
        },
        "actor": {
            "tokiUserId": user.id.as_i32(),
            "displayName": user.full_name,
        }
    });
    if let Some(prompt) = body.prompt.as_deref() {
        agent_request["prompt"] = json!(prompt);
    }

    let run_value = proxy_agent_json(
        &app_state,
        reqwest::Method::POST,
        "/internal/runs",
        Some(agent_request),
    )
    .await?;
    let run = parse_agent_run_record(run_value.clone())?;
    let source = AdoWorkItemSourceMetadata {
        organization: body.source.organization.clone(),
        project: body.source.project.clone(),
        work_item_id: body.source.id.clone(),
    };
    let actor = AgentRunActorMetadata {
        user_id: user.id.as_i32(),
        display_name: user.full_name.clone(),
    };

    if let Err(err) = app_state
        .agent_run_repo
        .upsert_from_run(&run, &source, &actor)
        .await
    {
        tracing::error!(
            run_id = %run.id,
            "failed to index created agent run for work item: {err}"
        );
        return Err(err.into());
    }

    Ok(Json(run_value))
}

async fn get_agent_run(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if let Some(row) = app_state.agent_run_repo.get_by_run_id(&id).await? {
        authorize_agent_run_row(&app_state, &user, &row).await?;
        let run_value = refresh_agent_run(&app_state, &id).await?;
        return Ok(Json(run_value));
    }

    let run_value = refresh_agent_run_without_summary_update(&app_state, &id).await?;
    let run = parse_agent_run_record(run_value.clone())?;
    let Some(source) = source_metadata_from_agent_run(&run) else {
        return Err(ApiError::not_found("agent run is not indexed"));
    };
    ensure_user_has_project_access(&app_state, &user, &source.organization, &source.project)
        .await?;
    let actor = AgentRunActorMetadata {
        user_id: user.id.as_i32(),
        display_name: user.full_name.clone(),
    };
    app_state
        .agent_run_repo
        .upsert_from_run(&run, &source, &actor)
        .await?;

    Ok(Json(run_value))
}

async fn delete_agent_run(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<()>, ApiError> {
    let row = indexed_agent_run_row(&app_state, &id).await?;
    authorize_agent_run_row(&app_state, &user, &row).await?;

    proxy_agent_json(
        &app_state,
        reqwest::Method::DELETE,
        &format!("/internal/runs/{id}"),
        None,
    )
    .await?;
    app_state.agent_run_repo.delete_by_run_id(&id).await?;

    Ok(Json(()))
}

async fn get_agent_run_events(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = indexed_agent_run_row(&app_state, &id).await?;
    authorize_agent_run_row(&app_state, &user, &row).await?;

    proxy_agent_json(
        &app_state,
        reqwest::Method::GET,
        &format!("/internal/runs/{id}/events"),
        None,
    )
    .await
    .map(Json)
}

async fn send_agent_run_feedback(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AgentRunFeedbackBody>,
) -> Result<Json<Value>, ApiError> {
    let row = indexed_agent_run_row(&app_state, &id).await?;
    authorize_agent_run_row(&app_state, &user, &row).await?;
    let run_value = proxy_agent_json(
        &app_state,
        reqwest::Method::POST,
        &format!("/internal/runs/{id}/feedback"),
        Some(json!({
            "message": body.message,
            "actor": {
                "tokiUserId": user.id.as_i32(),
                "displayName": user.full_name,
            },
        })),
    )
    .await?;
    update_agent_run_summary_from_value(&app_state, &id, &run_value).await?;
    Ok(Json(run_value))
}

async fn approve_agent_run_plan(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = indexed_agent_run_row(&app_state, &id).await?;
    authorize_agent_run_row(&app_state, &user, &row).await?;
    let run_value = proxy_agent_json(
        &app_state,
        reqwest::Method::POST,
        &format!("/internal/runs/{id}/approve-plan"),
        None,
    )
    .await?;
    update_agent_run_summary_from_value(&app_state, &id, &run_value).await?;
    Ok(Json(run_value))
}

async fn cancel_agent_run(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let row = indexed_agent_run_row(&app_state, &id).await?;
    authorize_agent_run_row(&app_state, &user, &row).await?;
    let run_value = proxy_agent_json(
        &app_state,
        reqwest::Method::POST,
        &format!("/internal/runs/{id}/cancel"),
        None,
    )
    .await?;
    update_agent_run_summary_from_value(&app_state, &id, &run_value).await?;
    Ok(Json(run_value))
}

async fn latest_agent_runs_by_work_items(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<LatestAgentRunsByWorkItemsRequest>,
) -> Result<Json<LatestAgentRunsByWorkItemsResponse>, ApiError> {
    if body.source_provider != AZURE_DEVOPS_WORK_ITEM_SOURCE_PROVIDER {
        return Err(ApiError::bad_request("unsupported source provider"));
    }

    ensure_user_has_project_access(&app_state, &user, &body.organization, &body.project).await?;

    let mut seen = HashSet::new();
    let work_item_ids = body
        .work_item_ids
        .into_iter()
        .filter(|id| !id.is_empty() && seen.insert(id.clone()))
        .collect::<Vec<_>>();

    if work_item_ids.is_empty() {
        return Ok(Json(LatestAgentRunsByWorkItemsResponse {
            runs: Vec::new(),
        }));
    }

    let rows = app_state
        .agent_run_repo
        .get_latest_by_work_items(
            &body.source_provider,
            &body.organization,
            &body.project,
            &work_item_ids,
        )
        .await?;

    let runs = stream::iter(rows.into_iter().map(|row| {
        let app_state = app_state.clone();
        async move {
            match refresh_agent_run(&app_state, &row.run_id).await {
                Ok(run_value) => match serde_json::from_value::<AgentRunRecord>(run_value) {
                    Ok(run) => summary_response_from_run(&run, &row, AgentRunSyncState::Fresh),
                    Err(err) => {
                        tracing::warn!(
                            run_id = %row.run_id,
                            "failed to parse refreshed latest agent run: {err}"
                        );
                        summary_response_from_row(row, AgentRunSyncState::Stale)
                    }
                },
                Err(err) => {
                    tracing::warn!(
                        run_id = %row.run_id,
                        "failed to refresh latest agent run summary: {err}"
                    );
                    summary_response_from_row(row, AgentRunSyncState::Stale)
                }
            }
        }
    }))
    .buffer_unordered(8)
    .collect::<Vec<_>>()
    .await;

    Ok(Json(LatestAgentRunsByWorkItemsResponse { runs }))
}

async fn ensure_user_has_project_access(
    app_state: &AppState,
    user: &AuthUser,
    organization: &str,
    project: &str,
) -> Result<(), ApiError> {
    let available_projects = app_state
        .work_item_factory
        .get_available_projects(user.id)
        .await?;
    let has_access = has_project_access(&available_projects, organization, project);

    if has_access {
        Ok(())
    } else {
        Err(ApiError::forbidden("You don't have access to this project"))
    }
}

async fn ensure_user_follows_repo(
    app_state: &AppState,
    user: &AuthUser,
    target_repo: &AgentRunTargetRepo,
) -> Result<(), ApiError> {
    let repo_key = RepoKey::new(
        &target_repo.organization,
        &target_repo.project,
        &target_repo.repo_name,
    );
    let followed_repos = app_state.user_repo.followed_repositories(user.id).await?;

    if followed_repos.contains(&repo_key) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "You must follow the target repository before launching an agent",
        ))
    }
}

fn target_repo_key(target_repo: &AgentRunTargetRepo) -> RepoKey {
    RepoKey::new(
        &target_repo.organization,
        &target_repo.project,
        &target_repo.repo_name,
    )
}

fn has_project_access(
    available_projects: &[WorkItemProject],
    organization: &str,
    project: &str,
) -> bool {
    available_projects.iter().any(|candidate| {
        candidate.organization.eq_ignore_ascii_case(organization)
            && candidate.project.eq_ignore_ascii_case(project)
    })
}

fn azure_devops_clone_url(target_repo: &AgentRunTargetRepo) -> Result<String, ApiError> {
    match target_repo.provider {
        AgentRunTargetRepoProvider::AzureDevOps => {
            let mut url = Url::parse("https://dev.azure.com").map_err(|err| {
                ApiError::internal(format!("invalid Azure DevOps base URL: {err}"))
            })?;
            url.path_segments_mut()
                .map_err(|_| ApiError::internal("Azure DevOps base URL cannot be a base"))?
                .extend([
                    target_repo.organization.as_str(),
                    target_repo.project.as_str(),
                    "_git",
                    target_repo.repo_name.as_str(),
                ]);
            Ok(url.to_string())
        }
    }
}

fn azure_devops_pull_request_web_url(
    organization: &str,
    project: &str,
    repo_name: &str,
    pull_request_id: i64,
) -> Result<String, ApiError> {
    let pull_request_id = pull_request_id.to_string();
    let mut url = Url::parse("https://dev.azure.com")
        .map_err(|err| ApiError::internal(format!("invalid Azure DevOps base URL: {err}")))?;
    url.path_segments_mut()
        .map_err(|_| ApiError::internal("Azure DevOps base URL cannot be a base"))?
        .extend([
            organization,
            project,
            "_git",
            repo_name,
            "pullrequest",
            &pull_request_id,
        ]);
    Ok(url.to_string())
}

async fn proxy_agent_json(
    app_state: &AppState,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, ApiError> {
    let base_url = app_state
        .agent_settings
        .base_url
        .as_deref()
        .ok_or_else(|| ApiError::internal("TOKI_AGENT__BASE_URL is not configured"))?;
    let internal_token = app_state
        .agent_settings
        .internal_token
        .as_deref()
        .ok_or_else(|| ApiError::internal("TOKI_AGENT__INTERNAL_TOKEN is not configured"))?;
    let url = build_agent_url(base_url, path)?;
    let request = app_state
        .http_client
        .request(method, url)
        .bearer_auth(internal_token);
    let request = if let Some(body) = body {
        request.json(&body)
    } else {
        request
    };
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::internal(format!("failed to reach toki-agent: {err}")))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|err| ApiError::internal(format!("invalid toki-agent JSON response: {err}")))?;

    if status.is_success() {
        Ok(body)
    } else {
        Err(agent_error(status, body))
    }
}

fn build_agent_url(base_url: &str, path: &str) -> Result<Url, ApiError> {
    let mut url = Url::parse(base_url)
        .map_err(|err| ApiError::internal(format!("invalid TOKI_AGENT__BASE_URL: {err}")))?;
    url.set_path(path);
    Ok(url)
}

fn agent_error(status: StatusCode, body: Value) -> ApiError {
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| body.get("error").and_then(Value::as_str))
        .unwrap_or("toki-agent request failed");

    ApiError::new(status, message)
}

fn parse_agent_run_record(run_value: Value) -> Result<AgentRunRecord, ApiError> {
    serde_json::from_value(run_value)
        .map_err(|err| ApiError::internal(format!("invalid agent run response: {err}")))
}

async fn indexed_agent_run_row(
    app_state: &AppState,
    run_id: &str,
) -> Result<AgentRunWorkItemRow, ApiError> {
    app_state
        .agent_run_repo
        .get_by_run_id(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent run is not indexed"))
}

async fn authorize_agent_run_row(
    app_state: &AppState,
    user: &AuthUser,
    row: &AgentRunWorkItemRow,
) -> Result<(), ApiError> {
    ensure_user_has_project_access(
        app_state,
        user,
        &row.source_organization,
        &row.source_project,
    )
    .await
}

async fn refresh_agent_run(app_state: &AppState, run_id: &str) -> Result<Value, ApiError> {
    let run_value = refresh_agent_run_without_summary_update(app_state, run_id).await?;
    update_agent_run_summary_from_value(app_state, run_id, &run_value).await?;
    Ok(run_value)
}

async fn refresh_agent_run_without_summary_update(
    app_state: &AppState,
    run_id: &str,
) -> Result<Value, ApiError> {
    let run = proxy_agent_json(
        app_state,
        reqwest::Method::GET,
        &format!("/internal/runs/{run_id}"),
        None,
    )
    .await?;

    maybe_publish_agent_run(app_state, run).await
}

async fn update_agent_run_summary_from_value(
    app_state: &AppState,
    run_id: &str,
    run_value: &Value,
) -> Result<(), ApiError> {
    let run = parse_agent_run_record(run_value.clone())?;
    app_state
        .agent_run_repo
        .update_synced_summary(run_id, &run.status, run.draft_pr_url(), run.updated_at)
        .await?;
    Ok(())
}

fn source_metadata_from_agent_run(run: &AgentRunRecord) -> Option<AdoWorkItemSourceMetadata> {
    parse_azure_devops_work_item_url(&run.source.url).map(|(organization, project, id)| {
        AdoWorkItemSourceMetadata {
            organization,
            project,
            work_item_id: id.unwrap_or_else(|| run.source.id.clone()),
        }
    })
}

fn parse_azure_devops_work_item_url(url: &str) -> Option<(String, String, Option<String>)> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?;
    let segments = url
        .path_segments()?
        .map(urlencoding::decode)
        .collect::<Result<Vec<_>, _>>()
        .ok()?
        .into_iter()
        .map(|part| part.into_owned())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if host.eq_ignore_ascii_case("dev.azure.com") {
        let organization = segments.first()?.clone();
        let project = segments.get(1)?.clone();
        return Some((organization, project, work_item_id_from_segments(&segments)));
    }

    if let Some(organization) = host.strip_suffix(".visualstudio.com") {
        let project = segments.first()?.clone();
        return Some((
            organization.to_string(),
            project,
            work_item_id_from_segments(&segments),
        ));
    }

    None
}

fn work_item_id_from_segments(segments: &[String]) -> Option<String> {
    segments
        .windows(2)
        .find(|window| window[0].eq_ignore_ascii_case("edit"))
        .map(|window| window[1].clone())
        .or_else(|| segments.last().cloned())
}

fn summary_response_from_run(
    run: &AgentRunRecord,
    row: &AgentRunWorkItemRow,
    sync_state: AgentRunSyncState,
) -> AgentRunIssueSummaryResponse {
    AgentRunIssueSummaryResponse {
        id: run.id.clone(),
        work_item_id: row.source_work_item_id.clone(),
        status: run.status.clone(),
        draft_pr_url: run.draft_pr_url().map(ToString::to_string),
        created_by: AgentRunIssueSummaryCreatedBy {
            display_name: row.created_by_display_name.clone(),
        },
        created_at: format_offset_date_time(run.created_at),
        updated_at: format_offset_date_time(run.updated_at),
        last_synced_at: format_offset_date_time(OffsetDateTime::now_utc()),
        sync_state,
    }
}

fn summary_response_from_row(
    row: AgentRunWorkItemRow,
    sync_state: AgentRunSyncState,
) -> AgentRunIssueSummaryResponse {
    let summary = AgentRunIssueSummary::from(row);
    AgentRunIssueSummaryResponse {
        id: summary.run_id,
        work_item_id: summary.source_work_item_id,
        status: summary.last_status,
        draft_pr_url: summary.draft_pr_url,
        created_by: AgentRunIssueSummaryCreatedBy {
            display_name: summary.created_by_display_name,
        },
        created_at: format_offset_date_time(summary.run_created_at),
        updated_at: format_offset_date_time(summary.run_updated_at),
        last_synced_at: format_offset_date_time(summary.last_synced_at),
        sync_state,
    }
}

fn format_offset_date_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|err| {
        tracing::warn!("failed to format timestamp: {err}");
        value.unix_timestamp().to_string()
    })
}

async fn maybe_publish_agent_run(
    app_state: &AppState,
    run_value: Value,
) -> Result<Value, ApiError> {
    let run = match serde_json::from_value::<AgentRunRecord>(run_value.clone()) {
        Ok(run) => run,
        Err(_) => return Ok(run_value),
    };

    if run.status != "awaitingBackendPublish" {
        return Ok(run_value);
    }

    let claimed = proxy_agent_json(
        app_state,
        reqwest::Method::POST,
        &format!("/internal/runs/{}/claim-publish", run.id),
        None,
    )
    .await?;
    let claimed_run = serde_json::from_value::<AgentRunRecord>(claimed.clone())
        .map_err(|err| ApiError::internal(format!("invalid claimed agent run: {err}")))?;

    if claimed_run.status != "backendPublishing" {
        return Ok(claimed);
    }

    match publish_agent_run(&claimed_run).await {
        Ok(draft_pr_url) => {
            proxy_agent_json(
                app_state,
                reqwest::Method::POST,
                &format!("/internal/runs/{}/complete-publish", claimed_run.id),
                Some(json!({ "draftPrUrl": draft_pr_url })),
            )
            .await
        }
        Err(err) => {
            proxy_agent_json(
                app_state,
                reqwest::Method::POST,
                &format!("/internal/runs/{}/fail-publish", claimed_run.id),
                Some(json!({ "message": err.to_string() })),
            )
            .await
        }
    }
}

async fn publish_agent_run(run: &AgentRunRecord) -> Result<String, ApiError> {
    let pending = run
        .pending_publish
        .as_ref()
        .ok_or_else(|| ApiError::internal("agent run is missing pending publish payload"))?;
    let target_repo = &run.target_repo;
    let organization =
        required_agent_target_field(target_repo.organization.as_deref(), "organization")?;
    let project = required_agent_target_field(target_repo.project.as_deref(), "project")?;
    let repo_name = required_agent_target_field(target_repo.repo_name.as_deref(), "repoName")?;
    let publish_dir = create_publish_dir(&run.id).await?;
    let result = publish_agent_run_in_dir(
        &publish_dir,
        &run.source,
        target_repo,
        pending,
        organization,
        project,
        repo_name,
    )
    .await;

    if let Err(err) = fs::remove_dir_all(&publish_dir).await {
        tracing::warn!(path = %publish_dir.display(), "failed to clean agent publish directory: {err}");
    }

    result
}

async fn publish_agent_run_in_dir(
    publish_dir: &FsPath,
    source: &AgentRunSource,
    target_repo: &AgentRunRecordTargetRepo,
    pending: &BackendPublishPayload,
    organization: &str,
    project: &str,
    repo_name: &str,
) -> Result<String, ApiError> {
    run_command(
        "git",
        &[
            "clone",
            "--depth",
            "1",
            "--single-branch",
            "--branch",
            &target_repo.default_branch,
            "--no-tags",
            &target_repo.clone_url,
            ".",
        ],
        publish_dir,
    )
    .await?;

    let head = run_command("git", &["rev-parse", "HEAD"], publish_dir)
        .await?
        .stdout
        .trim()
        .to_string();

    if head != pending.base_object_id {
        return Err(ApiError::conflict(format!(
            "target branch moved before publish: expected {}, got {}",
            pending.base_object_id, head
        )));
    }

    run_command(
        "git",
        &["checkout", "-b", &pending.branch_name],
        publish_dir,
    )
    .await?;

    for change in &pending.changes {
        apply_publish_change(publish_dir, change).await?;
    }

    run_command("git", &["config", "user.name", "Toki Agent"], publish_dir).await?;
    run_command(
        "git",
        &["config", "user.email", "agent@toki.local"],
        publish_dir,
    )
    .await?;
    run_command("git", &["add", "-A"], publish_dir).await?;
    run_command("git", &["commit", "-m", &pending.title], publish_dir).await?;
    run_command(
        "git",
        &[
            "push",
            "--set-upstream",
            "--force",
            "origin",
            &format!("HEAD:refs/heads/{}", pending.branch_name),
        ],
        publish_dir,
    )
    .await?;

    create_azure_devops_draft_pr(
        publish_dir,
        source,
        organization,
        project,
        repo_name,
        &target_repo.default_branch,
        pending,
    )
    .await
}

async fn create_azure_devops_draft_pr(
    cwd: &FsPath,
    source: &AgentRunSource,
    organization: &str,
    project: &str,
    repo_name: &str,
    default_branch: &str,
    pending: &BackendPublishPayload,
) -> Result<String, ApiError> {
    let args = build_azure_devops_pr_create_args(
        source,
        organization,
        project,
        repo_name,
        default_branch,
        pending,
    );
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_command("az", &arg_refs, cwd).await?;
    let body: Value = serde_json::from_str(&output.stdout)
        .map_err(|err| ApiError::internal(format!("invalid Azure CLI PR JSON: {err}")))?;

    if let Some(id) = body.get("pullRequestId").and_then(Value::as_i64) {
        return azure_devops_pull_request_web_url(organization, project, repo_name, id);
    }

    if let Some(url) = body.get("url").and_then(Value::as_str) {
        return Ok(url.to_string());
    }

    Err(ApiError::internal(
        "Azure CLI PR response did not include a URL",
    ))
}

fn build_azure_devops_pr_create_args(
    source: &AgentRunSource,
    organization: &str,
    project: &str,
    repo_name: &str,
    default_branch: &str,
    pending: &BackendPublishPayload,
) -> Vec<String> {
    let mut args = vec![
        "repos".to_string(),
        "pr".to_string(),
        "create".to_string(),
        "--organization".to_string(),
        format!("https://dev.azure.com/{organization}"),
        "--project".to_string(),
        project.to_string(),
        "--repository".to_string(),
        repo_name.to_string(),
        "--source-branch".to_string(),
        pending.branch_name.clone(),
        "--target-branch".to_string(),
        default_branch.to_string(),
        "--title".to_string(),
        pending.title.clone(),
        "--description".to_string(),
        pending.description.clone(),
        "--draft".to_string(),
    ];

    if source.id.chars().all(|ch| ch.is_ascii_digit()) {
        args.extend(["--work-items".to_string(), source.id.clone()]);
    }

    args.extend(["--output".to_string(), "json".to_string()]);
    args
}

async fn apply_publish_change(
    publish_dir: &FsPath,
    change: &BackendPublishChange,
) -> Result<(), ApiError> {
    let relative_path = safe_publish_relative_path(&change.path)?;
    let target_path = publish_dir.join(relative_path);

    match change.change_type {
        BackendPublishChangeType::Delete => {
            if fs::try_exists(&target_path).await.map_err(|err| {
                ApiError::internal(format!(
                    "failed to inspect {}: {err}",
                    target_path.display()
                ))
            })? {
                fs::remove_file(&target_path).await.map_err(|err| {
                    ApiError::internal(format!("failed to delete {}: {err}", target_path.display()))
                })?;
            }
        }
        BackendPublishChangeType::Add | BackendPublishChangeType::Edit => {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).await.map_err(|err| {
                    ApiError::internal(format!("failed to create {}: {err}", parent.display()))
                })?;
            }
            fs::write(&target_path, change.content.as_deref().unwrap_or(""))
                .await
                .map_err(|err| {
                    ApiError::internal(format!("failed to write {}: {err}", target_path.display()))
                })?;
        }
    }

    Ok(())
}

fn safe_publish_relative_path(value: &str) -> Result<PathBuf, ApiError> {
    let path = FsPath::new(value);

    if path.is_absolute() || value.split('/').any(|part| part == "..") {
        return Err(ApiError::bad_request(format!(
            "unsafe publish path: {value}"
        )));
    }

    Ok(path.to_path_buf())
}

async fn create_publish_dir(run_id: &str) -> Result<PathBuf, ApiError> {
    let sanitized_run_id: String = run_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .take(64)
        .collect();
    let dir = std::env::temp_dir().join(format!(
        "toki-agent-publish-{}-{}",
        std::process::id(),
        sanitized_run_id
    ));

    if fs::try_exists(&dir)
        .await
        .map_err(|err| ApiError::internal(format!("failed to inspect {}: {err}", dir.display())))?
    {
        fs::remove_dir_all(&dir).await.map_err(|err| {
            ApiError::internal(format!("failed to reset {}: {err}", dir.display()))
        })?;
    }

    fs::create_dir_all(&dir)
        .await
        .map_err(|err| ApiError::internal(format!("failed to create {}: {err}", dir.display())))?;

    Ok(dir)
}

struct CommandOutput {
    stdout: String,
}

async fn run_command(
    command: &str,
    args: &[&str],
    cwd: &FsPath,
) -> Result<CommandOutput, ApiError> {
    let output = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|err| {
            ApiError::internal(format!(
                "failed to run {} in {}: {err}",
                command,
                cwd.display()
            ))
        })?;

    if !output.status.success() {
        return Err(ApiError::internal(format!(
            "{} {} failed with status {}:\n{}{}",
            command,
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

fn required_agent_target_field<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, ApiError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::internal(format!("agent target repo missing {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azure_devops_clone_url_encodes_path_segments() {
        let result = azure_devops_clone_url(&AgentRunTargetRepo {
            provider: AgentRunTargetRepoProvider::AzureDevOps,
            organization: "org".to_string(),
            project: "project with space".to_string(),
            repo_name: "repo".to_string(),
            default_branch: "main".to_string(),
        });
        let url = match result {
            Ok(url) => url,
            Err(err) => panic!("{err}"),
        };

        assert_eq!(
            url,
            "https://dev.azure.com/org/project%20with%20space/_git/repo"
        );
    }

    #[test]
    fn azure_devops_pull_request_web_url_encodes_path_segments() {
        let result = azure_devops_pull_request_web_url(
            "lerumsdjur",
            "Lerums Djursjukhus",
            "LD.Apport",
            2647,
        );
        let url = match result {
            Ok(url) => url,
            Err(err) => panic!("{err}"),
        };

        assert_eq!(
            url,
            "https://dev.azure.com/lerumsdjur/Lerums%20Djursjukhus/_git/LD.Apport/pullrequest/2647"
        );
    }

    #[test]
    fn azure_devops_pr_create_args_link_numeric_work_item() {
        let args = build_azure_devops_pr_create_args(
            &AgentRunSource {
                id: "1619".to_string(),
                url: "https://dev.azure.com/lerumsdjur/Lerums%20Djursjukhus/_workitems/edit/1619"
                    .to_string(),
            },
            "lerumsdjur",
            "Lerums Djursjukhus",
            "LD.Apport",
            "main",
            &BackendPublishPayload {
                branch_name: "agent/adoWorkItem-1619-se-over-articlegrouping".to_string(),
                base_object_id: "abc123".to_string(),
                title: "Agent: Se över ArticleGrouping i frontend".to_string(),
                description: "## Summary\n\n- Changed matching to use IDs.".to_string(),
                changes: Vec::new(),
            },
        );

        assert_eq!(
            args,
            vec![
                "repos",
                "pr",
                "create",
                "--organization",
                "https://dev.azure.com/lerumsdjur",
                "--project",
                "Lerums Djursjukhus",
                "--repository",
                "LD.Apport",
                "--source-branch",
                "agent/adoWorkItem-1619-se-over-articlegrouping",
                "--target-branch",
                "main",
                "--title",
                "Agent: Se över ArticleGrouping i frontend",
                "--description",
                "## Summary\n\n- Changed matching to use IDs.",
                "--draft",
                "--work-items",
                "1619",
                "--output",
                "json",
            ]
        );
    }

    #[test]
    fn build_agent_url_replaces_base_path() {
        let result = build_agent_url("https://agent.example/base", "/internal/runs/123");
        let url = match result {
            Ok(url) => url,
            Err(err) => panic!("{err}"),
        };

        assert_eq!(url.as_str(), "https://agent.example/internal/runs/123");
    }

    #[test]
    fn parses_dev_azure_work_item_url() {
        let parsed = parse_azure_devops_work_item_url(
            "https://dev.azure.com/Lerum/IT%20och%20digitalisering/_workitems/edit/3838",
        );

        assert_eq!(
            parsed,
            Some((
                "Lerum".to_string(),
                "IT och digitalisering".to_string(),
                Some("3838".to_string())
            ))
        );
    }

    #[test]
    fn parses_visualstudio_work_item_url() {
        let parsed = parse_azure_devops_work_item_url(
            "https://example.visualstudio.com/Project/_workitems/edit/42",
        );

        assert_eq!(
            parsed,
            Some((
                "example".to_string(),
                "Project".to_string(),
                Some("42".to_string())
            ))
        );
    }
}
