use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use azure_devops_rust_api::{
    core,
    git::{self, models::GitCommitRef},
    graph::{self, models::GraphUser},
    wit::{
        self,
        models::{
            work_item_batch_get_request::Expand, Wiql, WorkItemBatchGetRequest,
            WorkItemClassificationNode,
        },
    },
    work, Credential,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::Semaphore;
use tracing::debug;

use crate::{Identity, Iteration, PullRequest, Thread, WorkItem, WorkItemComment};

const WIQL_QUERY_TIMEOUT: Duration = Duration::from_secs(8);
const WIQL_API_VERSION: &str = "7.1-preview";

#[derive(Debug, thiserror::Error)]
pub enum RepoClientError {
    #[error("Azure DevOps API error: {0}")]
    AzureDevOpsError(#[from] typespec::error::Error),
    #[error("Azure Core API error: {0}")]
    AzureCoreError(#[from] azure_core::Error),
    #[error("Azure DevOps HTTP error (status {status}): {body}")]
    HttpStatus { status: u16, body: String },
    #[error("Repository not found: {0}")]
    RepoNotFound(String),
    #[error("Response payload exceeds {max_bytes} bytes (actual: {actual_bytes} bytes)")]
    PayloadTooLarge { actual_bytes: u64, max_bytes: usize },
}

#[derive(Serialize)]
struct ProjectScopeWiqlBody<'a> {
    query: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectScopeWiqlResult {
    #[serde(default)]
    work_items: Vec<ProjectScopeWiqlWorkItemRef>,
}

#[derive(Deserialize)]
struct ProjectScopeWiqlWorkItemRef {
    id: Option<i32>,
}

#[derive(Clone)]
pub struct RepoClient {
    core_client: core::Client,
    git_client: git::Client,
    work_item_client: wit::Client,
    work_client: work::Client,
    graph_client: graph::Client,
    http_client: reqwest::Client,
    organization: String,
    project: String,
    repo_name: String,
    repo_id: String,
    pat: String,
}

impl RepoClient {
    pub async fn new(
        repo_name: &str,
        organization: &str,
        project: &str,
        pat: &str,
    ) -> Result<Self, RepoClientError> {
        // might need to disable retries or set a timeout (https://docs.rs/azure_devops_rust_api/latest/azure_devops_rust_api/git/struct.ClientBuilder.html, https://docs.rs/azure_core/0.20.0/azure_core/struct.TimeoutPolicy.html)
        let credential = Credential::from_pat(pat.to_owned());
        let core_client = core::ClientBuilder::new(credential.clone()).build();
        let git_client = git::ClientBuilder::new(credential.clone()).build();
        let work_item_client = wit::ClientBuilder::new(credential.clone()).build();
        let work_client = work::ClientBuilder::new(credential.clone()).build();
        let graph_client = graph::ClientBuilder::new(credential).build();
        let http_client = reqwest::Client::new();

        let repo = git_client
            .repositories_client()
            .list(organization, project)
            .await?
            .value
            .iter()
            .find(|repo| repo.name == repo_name)
            .cloned()
            .ok_or_else(|| RepoClientError::RepoNotFound(repo_name.to_string()))?;

        Ok(Self {
            core_client,
            git_client,
            work_item_client,
            work_client,
            graph_client,
            http_client,
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_name: repo.name,
            repo_id: repo.id,
            pat: pat.to_owned(),
        })
    }

    pub fn organization(&self) -> &str {
        &self.organization
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn repo_name(&self) -> &str {
        &self.repo_name
    }

    pub fn repo_id(&self) -> &str {
        &self.repo_id
    }

    pub async fn get_open_pull_requests(&self) -> Result<Vec<PullRequest>, RepoClientError> {
        let pull_requests = self
            .git_client
            .pull_requests_client()
            .get_pull_requests(&self.organization, &self.repo_id, &self.project)
            .await?
            .value;

        Ok(pull_requests.into_iter().map(PullRequest::from).collect())
    }

    pub async fn get_all_pull_requests(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<PullRequest>, RepoClientError> {
        const PAGE_SIZE: i32 = 50;
        let max_items = limit.unwrap_or(usize::MAX);

        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut pull_requests = Vec::new();
        let mut skip = 0;

        loop {
            let page = self
                .git_client
                .pull_requests_client()
                .get_pull_requests(&self.organization, &self.repo_id, &self.project)
                .search_criteria_status("all")
                .skip(skip)
                .top(PAGE_SIZE)
                .await?
                .value;

            if page.is_empty() {
                break;
            }

            let remaining_capacity = max_items.saturating_sub(pull_requests.len());
            pull_requests.extend(
                page.into_iter()
                    .take(remaining_capacity)
                    .map(PullRequest::from),
            );

            if pull_requests.len() >= max_items {
                break;
            }

            skip += PAGE_SIZE;
        }

        Ok(pull_requests)
    }

    pub async fn get_threads_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<Thread>, RepoClientError> {
        let threads = self
            .git_client
            .pull_request_threads_client()
            .list(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(threads
            .into_iter()
            .map(|t| Thread::from(t.comment_thread))
            .collect())
    }

    pub async fn get_work_item_ids_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<i32>, RepoClientError> {
        let work_item_refs = self
            .git_client
            .pull_request_work_items_client()
            .list(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(work_item_refs
            .into_iter()
            .filter_map(|r| r.id)
            .filter_map(|id| id.parse().ok())
            .collect())
    }

    pub async fn get_commits_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<GitCommitRef>, RepoClientError> {
        let commits = self
            .git_client
            .pull_request_commits_client()
            .get_pull_request_commits(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(commits)
    }

    /// Fetch comments on a work item.
    ///
    /// The SDK's `CommentList` deserialization can fail on empty responses because
    /// `WorkItemTrackingResourceReference::url` is required but the API omits it.
    /// We catch deserialization errors and return an empty vec as a fallback.
    pub async fn get_work_item_comments(
        &self,
        work_item_id: i32,
    ) -> Result<Vec<WorkItemComment>, RepoClientError> {
        let result = self
            .work_item_client
            .comments_client()
            .get_comments(&self.organization, &self.project, work_item_id)
            .await;

        match result {
            Ok(comment_list) => Ok(comment_list
                .comments
                .into_iter()
                .map(WorkItemComment::from)
                .collect()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("deserialize") {
                    tracing::debug!(
                        work_item_id,
                        "Comments deserialization failed (likely empty), returning empty vec"
                    );
                    Ok(vec![])
                } else {
                    Err(e.into())
                }
            }
        }
    }

    pub async fn get_work_items(&self, ids: Vec<i32>) -> Result<Vec<WorkItem>, RepoClientError> {
        const BATCH_SIZE: usize = 200;

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_work_items = Vec::with_capacity(ids.len());

        for chunk in ids.chunks(BATCH_SIZE) {
            let mut batch_request = WorkItemBatchGetRequest::new();
            batch_request.expand = Some(Expand::Relations);
            batch_request.ids = chunk.to_vec();

            let work_items = self
                .work_item_client
                .work_items_client()
                .get_work_items_batch(&self.organization, batch_request, &self.project)
                .await?
                .value;

            all_work_items.extend(work_items.into_iter().map(WorkItem::from));
        }

        Ok(all_work_items)
    }

    /// Download a work item attachment by ID.
    pub async fn get_work_item_attachment(
        &self,
        attachment_id: &str,
        file_name: Option<&str>,
        max_download_bytes: Option<usize>,
    ) -> Result<(Vec<u8>, Option<String>), RepoClientError> {
        let mut request = self
            .work_item_client
            .attachments_client()
            .get(&self.organization, attachment_id, &self.project)
            .download(true);

        if let Some(file_name) = file_name {
            request = request.file_name(file_name);
        }

        let raw_response = request.send().await?.into_raw_response();
        let mut content_type = None;
        let mut content_length = None;
        for (name, value) in raw_response.headers().iter() {
            if name.as_str().eq_ignore_ascii_case("content-type") {
                content_type = Some(value.as_str().to_string());
                continue;
            }

            if name.as_str().eq_ignore_ascii_case("content-length") {
                content_length = value.as_str().parse::<u64>().ok();
            }
        }

        if let (Some(max_bytes), Some(actual_bytes)) = (max_download_bytes, content_length) {
            if actual_bytes > max_bytes as u64 {
                return Err(RepoClientError::PayloadTooLarge {
                    actual_bytes,
                    max_bytes,
                });
            }
        }

        let bytes: azure_core::Bytes = raw_response.into_raw_body().collect().await?;
        if let Some(max_bytes) = max_download_bytes {
            if bytes.len() > max_bytes {
                return Err(RepoClientError::PayloadTooLarge {
                    actual_bytes: bytes.len() as u64,
                    max_bytes,
                });
            }
        }

        Ok((bytes.to_vec(), content_type))
    }

    /// Query work item IDs using WIQL (Work Item Query Language).
    ///
    /// The `team` parameter is required for WIQL macros like `@currentIteration`.
    /// If unsure, pass the project name as the team.
    pub async fn query_work_item_ids_wiql(
        &self,
        query: &str,
        team: &str,
    ) -> Result<Vec<i32>, RepoClientError> {
        let wiql = Wiql {
            query: Some(query.to_string()),
        };

        let result = tokio::time::timeout(
            WIQL_QUERY_TIMEOUT,
            self.work_item_client.wiql_client().query_by_wiql(
                &self.organization,
                wiql,
                &self.project,
                team,
            ),
        )
        .await
        .map_err(|_| internal_http_error("WIQL query request timed out"))??;

        let ids: Vec<i32> = result.work_items.into_iter().filter_map(|r| r.id).collect();

        debug!(
            "WIQL query returned {} work item IDs for {}/{}",
            ids.len(),
            self.organization,
            self.project
        );

        Ok(ids)
    }

    /// Query work item IDs using WIQL at project scope (no team segment in URL).
    ///
    /// This avoids team-specific WIQL routing issues for projects where the default
    /// "{Project} Team" does not exist.
    pub async fn query_work_item_ids_wiql_project_scope(
        &self,
        query: &str,
    ) -> Result<Vec<i32>, RepoClientError> {
        let mut url = reqwest::Url::parse("https://dev.azure.com")
            .map_err(|error| internal_http_error(format!("Failed to build WIQL URL: {error}")))?;
        url.path_segments_mut()
            .map_err(|_| internal_http_error("Failed to build WIQL URL path"))?
            .extend([
                self.organization.as_str(),
                self.project.as_str(),
                "_apis",
                "wit",
                "wiql",
            ]);
        url.query_pairs_mut()
            .append_pair("api-version", WIQL_API_VERSION);

        let response = self
            .http_client
            .post(url)
            .basic_auth("", Some(&self.pat))
            .timeout(WIQL_QUERY_TIMEOUT)
            .json(&ProjectScopeWiqlBody { query })
            .send()
            .await
            .map_err(|error| {
                internal_http_error(format!("Project WIQL request failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read response body>".to_string());
            return Err(RepoClientError::HttpStatus {
                status: status.as_u16(),
                body: body.chars().take(256).collect(),
            });
        }

        let result = response
            .json::<ProjectScopeWiqlResult>()
            .await
            .map_err(|error| {
                internal_http_error(format!("Failed to decode project WIQL response: {error}"))
            })?;
        let ids: Vec<i32> = result
            .work_items
            .into_iter()
            .filter_map(|item| item.id)
            .collect();

        debug!(
            "Project-scope WIQL query returned {} work item IDs for {}/{}",
            ids.len(),
            self.organization,
            self.project
        );

        Ok(ids)
    }

    /// Get all iterations for the project, flattened from the classification node tree.
    ///
    /// `depth` controls how deep to traverse the tree (defaults to 10).
    pub async fn get_iterations(
        &self,
        depth: Option<i32>,
    ) -> Result<Vec<Iteration>, RepoClientError> {
        let root_node = self
            .work_item_client
            .classification_nodes_client()
            .get(&self.organization, &self.project, "iterations", "")
            .depth(depth.unwrap_or(10))
            .await?;

        let mut iterations = Vec::new();
        flatten_classification_nodes(&root_node, &mut iterations);

        debug!(
            "Found {} iterations for {}/{}",
            iterations.len(),
            self.organization,
            self.project
        );

        Ok(iterations)
    }

    // TODO: how to handle continuation token?
    pub async fn get_graph_users(&self) -> Result<Vec<GraphUser>, RepoClientError> {
        let user_list_response = self
            .graph_client
            .users_client()
            .list(&self.organization)
            .await?;

        if user_list_response.count.is_none_or(|count| count == 0) {
            return Ok(vec![]);
        }

        Ok(user_list_response.value)
    }

    /// Workaround to get all identities as there is no way to list all identities with
    /// the same ID that is used in the git API.
    pub async fn get_git_identities(&self) -> Result<Vec<Identity>, RepoClientError> {
        const MAX_PULL_REQUESTS: usize = 100;
        const CONCURRENCY: usize = 10;

        let pull_requests = self.get_all_pull_requests(Some(MAX_PULL_REQUESTS)).await?;

        let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
        let mut handles = Vec::with_capacity(pull_requests.len());
        for pr in &pull_requests {
            let client = self.clone();
            let pr_id = pr.id;
            let semaphore = Arc::clone(&semaphore);
            handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                client.get_threads_in_pull_request(pr_id).await
            }));
        }

        let mut threads = Vec::new();
        for handle in handles {
            if let Ok(Ok(pr_threads)) = handle.await {
                threads.extend(pr_threads);
            }
        }

        let mut identities = HashSet::new();
        for pull_request in pull_requests {
            identities.insert(pull_request.created_by);
            pull_request.reviewers.iter().for_each(|reviewer| {
                identities.insert(reviewer.identity.clone());
            });
        }

        for thread in threads {
            thread.comments.iter().for_each(|comment| {
                identities.insert(comment.author.clone());
            });
        }

        Ok(identities.into_iter().collect())
    }

    /// Get team iterations (sprints) from the work API.
    ///
    /// Unlike `get_iterations()` which returns classification nodes with numeric IDs,
    /// this returns team-scoped iterations with GUID IDs that the taskboard API requires.
    pub async fn get_team_iterations(
        &self,
        team: &str,
    ) -> Result<Vec<TeamIteration>, RepoClientError> {
        let list = self
            .work_client
            .iterations_client()
            .list(&self.organization, &self.project, team)
            .await?;

        Ok(list
            .value
            .into_iter()
            .filter_map(|it| {
                Some(TeamIteration {
                    id: it.id?,
                    name: it.name.unwrap_or_default(),
                    path: it.path.unwrap_or_default(),
                })
            })
            .collect())
    }

    /// Get current team iterations using the Work API timeframe filter.
    ///
    /// This relies on Azure DevOps team's sprint settings (`$timeframe=current`)
    /// and is more reliable than deriving "current" solely from classification node dates.
    pub async fn get_current_team_iteration_paths(
        &self,
        team: &str,
    ) -> Result<Vec<String>, RepoClientError> {
        let list = self
            .work_client
            .iterations_client()
            .list(&self.organization, &self.project, team)
            .timeframe("current")
            .await?;

        Ok(list.value.into_iter().filter_map(|it| it.path).collect())
    }

    /// List available team names for this project.
    pub async fn get_project_team_names(&self) -> Result<Vec<String>, RepoClientError> {
        let teams = self
            .core_client
            .teams_client()
            .get_teams(&self.organization, &self.project)
            .await?;

        let mut names: Vec<String> = teams
            .value
            .into_iter()
            .filter_map(|team| team.web_api_team_ref.name)
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect();

        names.sort();
        names.dedup();

        debug!(
            project = %self.project,
            team_count = names.len(),
            "Got available teams for project"
        );

        Ok(names)
    }

    /// Get ordered taskboard column definitions for a given team.
    pub async fn get_taskboard_columns(
        &self,
        team: &str,
    ) -> Result<Vec<TaskboardColumnDefinition>, RepoClientError> {
        let response = self
            .work_client
            .taskboard_columns_client()
            .get(&self.organization, &self.project, team)
            .await?;

        let mut columns = Vec::with_capacity(response.columns.len());
        for (idx, column) in response.columns.into_iter().enumerate() {
            let Some(name) = column.name else {
                continue;
            };
            let trimmed_name = name.trim();
            if trimmed_name.is_empty() {
                continue;
            }

            columns.push(TaskboardColumnDefinition {
                id: column.id.filter(|id| !id.trim().is_empty()),
                name: trimmed_name.to_string(),
                order: column.order.unwrap_or((idx as i32) * 10),
            });
        }

        columns.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));

        debug!("Got {} taskboard columns for team={}", columns.len(), team,);

        Ok(columns)
    }

    /// Get taskboard work item column assignments for a given team and iteration.
    ///
    /// Returns a map of work_item_id → assigned column details.
    /// This uses the sprint taskboard API which has per-work-item column assignments
    /// that differ from `System.State` when the taskboard is customized.
    pub async fn get_taskboard_work_item_columns(
        &self,
        team: &str,
        iteration_id: &str,
    ) -> Result<HashMap<i32, TaskboardWorkItemColumnAssignment>, RepoClientError> {
        let items = self
            .work_client
            .taskboard_work_items_client()
            .list(&self.organization, &self.project, team, iteration_id)
            .await?;

        let mut map = HashMap::new();
        for item in items.value {
            if let (Some(id), Some(column_name)) = (item.work_item_id, item.column) {
                map.insert(
                    id,
                    TaskboardWorkItemColumnAssignment {
                        column_id: item.column_id,
                        column_name,
                        state: item.state,
                    },
                );
            }
        }

        debug!(
            "Got {} taskboard column assignments for team={}, iteration={}",
            map.len(),
            team,
            iteration_id
        );

        Ok(map)
    }

    /// Move a work item card to a new taskboard column.
    pub async fn move_taskboard_work_item_to_column(
        &self,
        team: &str,
        iteration_id: &str,
        work_item_id: i32,
        target_column_name: &str,
    ) -> Result<(), RepoClientError> {
        let mut body = work::models::UpdateTaskboardWorkItemColumn::new();
        body.new_column = Some(target_column_name.to_string());

        self.work_client
            .taskboard_work_items_client()
            .update(
                &self.organization,
                body,
                &self.project,
                team,
                iteration_id,
                work_item_id,
            )
            .await?;

        debug!(
            team = team,
            iteration_id = iteration_id,
            work_item_id = work_item_id,
            target_column = target_column_name,
            "Moved taskboard work item to new column"
        );

        Ok(())
    }
}

/// A team iteration from the work API, with a GUID ID.
#[derive(Clone, Debug)]
pub struct TeamIteration {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// A taskboard column definition.
#[derive(Clone, Debug)]
pub struct TaskboardColumnDefinition {
    pub id: Option<String>,
    pub name: String,
    pub order: i32,
}

/// Taskboard column assignment for a single work item.
#[derive(Clone, Debug)]
pub struct TaskboardWorkItemColumnAssignment {
    pub column_id: Option<String>,
    pub column_name: String,
    pub state: Option<String>,
}

/// Recursively flatten a `WorkItemClassificationNode` tree into a `Vec<Iteration>`.
///
/// Each node's `attributes` JSON may contain `startDate` and `finishDate` fields
/// as ISO 8601 date strings.
fn flatten_classification_nodes(
    node: &WorkItemClassificationNode,
    iterations: &mut Vec<Iteration>,
) {
    // Only include nodes that have an id (skip the root if it's just a container)
    if let Some(id) = node.id {
        let name = node.name.clone().unwrap_or_default();
        let path = node.path.clone().unwrap_or_default();

        let (start_date, finish_date) = extract_iteration_dates(&node.attributes);

        iterations.push(Iteration {
            id,
            name,
            path,
            start_date,
            finish_date,
        });
    }

    for child in &node.children {
        flatten_classification_nodes(child, iterations);
    }
}

/// Extract start and finish dates from the classification node's `attributes` JSON.
///
/// The attributes object may look like:
/// ```json
/// { "startDate": "2024-01-15T00:00:00Z", "finishDate": "2024-01-29T00:00:00Z" }
/// ```
fn extract_iteration_dates(
    attributes: &Option<serde_json::Value>,
) -> (Option<OffsetDateTime>, Option<OffsetDateTime>) {
    let Some(attrs) = attributes else {
        return (None, None);
    };

    let start_date = attrs
        .get("startDate")
        .and_then(|v| v.as_str())
        .and_then(parse_date_string);

    let finish_date = attrs
        .get("finishDate")
        .and_then(|v| v.as_str())
        .and_then(parse_date_string);

    (start_date, finish_date)
}

/// Parse a date string from Azure DevOps into an `OffsetDateTime`.
///
/// Supports ISO 8601 / RFC 3339 format (e.g. "2024-01-15T00:00:00Z").
fn parse_date_string(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

fn internal_http_error(body: impl Into<String>) -> RepoClientError {
    RepoClientError::HttpStatus {
        status: 500,
        body: body.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, Time};

    async fn get_repo_client() -> RepoClient {
        dotenvy::from_filename(".env.local")
            .or_else(|_| dotenvy::from_filename("az-devops/.env.local"))
            .ok();

        RepoClient::new(
            &std::env::var("ADO_REPO").unwrap(),
            &std::env::var("ADO_ORGANIZATION").unwrap(),
            &std::env::var("ADO_PROJECT").unwrap(),
            &std::env::var("ADO_TOKEN").unwrap(),
        )
        .await
        .unwrap()
    }

    fn effective_finish(finish: OffsetDateTime) -> OffsetDateTime {
        if finish.time() == Time::MIDNIGHT {
            finish + Duration::days(1) - Duration::nanoseconds(1)
        } else {
            finish
        }
    }

    fn is_iteration_current(iteration: &Iteration, now: OffsetDateTime) -> bool {
        match (iteration.start_date, iteration.finish_date) {
            (Some(start), Some(finish)) => now >= start && now <= effective_finish(finish),
            _ => false,
        }
    }

    fn normalize_column_name(name: &str) -> String {
        name.trim().to_ascii_lowercase()
    }

    async fn wait_for_column_assignment(
        repo_client: &RepoClient,
        team: &str,
        iteration_id: &str,
        work_item_id: i32,
        expected_column: &str,
    ) -> Result<Option<String>, RepoClientError> {
        let expected = normalize_column_name(expected_column);

        for attempt in 0..5 {
            let assignments = repo_client
                .get_taskboard_work_item_columns(team, iteration_id)
                .await?;

            if let Some(assignment) = assignments.get(&work_item_id) {
                let actual = normalize_column_name(&assignment.column_name);
                if actual == expected {
                    return Ok(Some(assignment.column_name.clone()));
                }
            }

            if attempt < 4 {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            }
        }

        Ok(None)
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_open_pull_requests() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        assert!(!pull_requests.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_pull_request_threads() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        let test_pr = &pull_requests[0];
        let threads = repo_client
            .get_threads_in_pull_request(test_pr.id)
            .await
            .unwrap();

        assert!(!threads.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_pull_request_commits() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        assert!(!pull_requests.is_empty());

        let test_pr = &pull_requests[0];
        let commits = repo_client
            .get_commits_in_pull_request(test_pr.id)
            .await
            .unwrap();

        assert!(!commits.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_work_items() {
        let repo_client = get_repo_client().await;

        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();
        assert!(!pull_requests.is_empty());

        let test_pr = pull_requests
            .iter()
            .find(|pr| pr.title == "Make export of sell prices more robust")
            .unwrap();

        let work_item_ids = repo_client
            .get_work_item_ids_in_pull_request(test_pr.id)
            .await
            .unwrap();
        assert!(!work_item_ids.is_empty());

        let work_items = repo_client.get_work_items(work_item_ids).await.unwrap();
        assert!(!work_items.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_wiql_query_with_iteration_path() {
        let repo_client = get_repo_client().await;

        let iterations = repo_client.get_iterations(None).await.unwrap();
        assert!(!iterations.is_empty());

        // Classification node paths use "\Project\Iteration\Sprint N" format,
        // but System.IterationPath on work items uses "Project\Sprint N".
        // Verify the normalized path works with WIQL.
        let iteration = iterations.last().unwrap();
        let raw_path = &iteration.path;
        let normalized =
            raw_path
                .strip_prefix('\\')
                .unwrap_or(raw_path)
                .replacen("\\Iteration\\", "\\", 1);

        println!("Raw path: '{raw_path}' -> Normalized: '{normalized}'");

        let query = format!(
            "SELECT [System.Id] FROM WorkItems \
             WHERE [System.TeamProject] = @project \
             AND [System.IterationPath] UNDER '{}' \
             ORDER BY [Microsoft.VSTS.Common.Priority] asc",
            normalized
        );

        // Empty team string works; the SDK omits it from the URL
        let ids = repo_client
            .query_work_item_ids_wiql(&query, "")
            .await
            .unwrap();

        println!("Found {} work items in '{normalized}'", ids.len());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_iterations_current_sprint_diagnostics() {
        let repo_client = get_repo_client().await;
        let team = format!("{} Team", repo_client.project());
        let now = OffsetDateTime::now_utc();

        let iterations = repo_client.get_iterations(None).await.unwrap();
        assert!(!iterations.is_empty(), "No iterations returned");

        let current_team_paths = repo_client
            .get_current_team_iteration_paths(&team)
            .await
            .unwrap();
        assert!(
            !current_team_paths.is_empty(),
            "No current team iteration paths returned for team '{team}'"
        );

        for path in &current_team_paths {
            println!("Current team iteration path from API: {path}");
        }

        let current: Vec<_> = iterations
            .iter()
            .filter(|iteration| is_iteration_current(iteration, now))
            .collect();

        for iteration in &current {
            println!(
                "Current iteration candidate: name='{}', path='{}', start={:?}, finish={:?}",
                iteration.name, iteration.path, iteration.start_date, iteration.finish_date
            );
        }

        if current.is_empty() {
            println!(
                "No current iteration found by date ranges. This indicates a date metadata gap \
                 (or different semantics), so team timeframe API should be used as source of truth."
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_current_team_iteration_paths() {
        let repo_client = get_repo_client().await;
        let team = format!("{} Team", repo_client.project());
        let current_paths = repo_client
            .get_current_team_iteration_paths(&team)
            .await
            .unwrap();

        for path in &current_paths {
            println!("Current team iteration path: {path}");
        }

        assert!(
            !current_paths.is_empty(),
            "No current team iteration paths returned for team '{team}'"
        );
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_graph_users() {
        let repo_client = get_repo_client().await;
        let identities = repo_client.get_graph_users().await.unwrap();
        assert!(!identities.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_git_identities() {
        let repo_client = get_repo_client().await;
        let identities = repo_client.get_git_identities().await.unwrap();

        for identity in &identities {
            println!(
                "Name: {}, Email: {}, ID: {}",
                identity.display_name, identity.unique_name, identity.id
            );
        }

        assert!(!identities.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_work_item_comments() {
        let repo_client = get_repo_client().await;

        // Use work item 3072 which we know exists from the failing request
        let comments = repo_client.get_work_item_comments(3072).await.unwrap();

        println!("Found {} comments on work item #3072", comments.len());
        for comment in &comments {
            println!(
                "  Comment #{}: by {} at {} (deleted={}), text={:.80}",
                comment.id,
                comment.author_name,
                comment.created_at,
                comment.is_deleted,
                comment.text,
            );
        }

        // Even if there are no comments, the call should succeed (not deserialize-fail)
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_get_work_item_comments_empty() {
        let repo_client = get_repo_client().await;

        // Fetch a work item that is unlikely to have comments — use WIQL to find one
        let ids = repo_client
            .query_work_item_ids_wiql(
                "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project ORDER BY [System.CreatedDate] asc",
                "",
            )
            .await
            .unwrap();

        // Try the first work item (oldest, likely no comments)
        if let Some(&id) = ids.first() {
            let comments = repo_client.get_work_item_comments(id).await.unwrap();
            println!(
                "Work item #{id}: {} comments (should handle empty gracefully)",
                comments.len()
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection"]
    async fn test_board_columns() {
        let repo_client = get_repo_client().await;

        let team = format!("{} Team", repo_client.project());

        // Get team iterations (with GUID IDs needed by taskboard API)
        let iterations = repo_client.get_team_iterations(&team).await.unwrap();
        assert!(!iterations.is_empty());

        let iteration = iterations.last().unwrap();
        println!(
            "Using iteration: {} (id: {}, path: {})",
            iteration.name, iteration.id, iteration.path
        );

        // Test the taskboard work item columns
        let columns = repo_client
            .get_taskboard_work_item_columns(&team, &iteration.id)
            .await
            .unwrap();

        println!("=== Taskboard work item columns ===");
        for (id, col) in &columns {
            println!(
                "  Work item #{id}: {} ({:?})",
                col.column_name, col.column_id
            );
        }

        assert!(!columns.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires real Azure DevOps connection and mutates taskboard state"]
    async fn test_move_taskboard_work_item_round_trip() {
        let repo_client = get_repo_client().await;
        let team = format!("{} Team", repo_client.project());
        let iterations = repo_client.get_team_iterations(&team).await.unwrap();
        assert!(
            !iterations.is_empty(),
            "No team iterations returned for team '{team}'"
        );

        let columns = repo_client.get_taskboard_columns(&team).await.unwrap();
        assert!(
            columns.len() >= 2,
            "Need at least two columns to test move operation"
        );

        let mut selected_iteration = None;
        let mut assignments = HashMap::new();
        for iteration in iterations.iter().rev() {
            let candidate_assignments = repo_client
                .get_taskboard_work_item_columns(&team, &iteration.id)
                .await
                .unwrap();
            if !candidate_assignments.is_empty() {
                selected_iteration = Some((iteration.id.clone(), iteration.name.clone()));
                assignments = candidate_assignments;
                break;
            }
        }
        let (iteration_id, iteration_name) = selected_iteration.expect(
            "No taskboard assignments available in any team iteration; cannot perform move test",
        );

        let candidate = assignments.iter().find_map(|(work_item_id, assignment)| {
            let state = assignment.state.as_deref()?.trim();
            if state.is_empty() {
                return None;
            }

            assignments
                .values()
                .find(|other| {
                    other
                        .state
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|other_state| {
                            other_state.eq_ignore_ascii_case(state)
                                && normalize_column_name(&other.column_name)
                                    != normalize_column_name(&assignment.column_name)
                        })
                })
                .map(|target| {
                    (
                        *work_item_id,
                        assignment.column_name.clone(),
                        target.column_name.clone(),
                        state.to_string(),
                    )
                })
        });

        let Some((work_item_id, original_column, target_column, state_name)) = candidate else {
            panic!("Could not find a same-state move candidate across different columns");
        };

        println!(
            "Round-trip move candidate: work item #{work_item_id}, state='{}', '{}' -> '{}' (team='{}', iteration='{}')",
            state_name, original_column, target_column, team, iteration_name
        );

        repo_client
            .move_taskboard_work_item_to_column(&team, &iteration_id, work_item_id, &target_column)
            .await
            .unwrap();

        let moved = wait_for_column_assignment(
            &repo_client,
            &team,
            &iteration_id,
            work_item_id,
            &target_column,
        )
        .await
        .unwrap()
        .is_some();

        let restore_result = repo_client
            .move_taskboard_work_item_to_column(
                &team,
                &iteration_id,
                work_item_id,
                &original_column,
            )
            .await;
        assert!(
            restore_result.is_ok(),
            "Failed to restore original column '{}': {:?}",
            original_column,
            restore_result.err()
        );

        let restored = wait_for_column_assignment(
            &repo_client,
            &team,
            &iteration_id,
            work_item_id,
            &original_column,
        )
        .await
        .unwrap()
        .is_some();

        assert!(
            moved,
            "Work item #{work_item_id} was not observed in target column '{target_column}' after move"
        );
        assert!(
            restored,
            "Work item #{work_item_id} was not restored to original column '{original_column}'"
        );
    }
}
