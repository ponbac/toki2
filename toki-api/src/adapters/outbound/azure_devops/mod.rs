mod conversions;
mod urls;

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use async_trait::async_trait;

use crate::domain::{
    models::{
        synthetic_column_id_from_name, BoardColumn, BoardColumnAssignment, Iteration, WorkItem,
        WorkItemComment,
    },
    ports::outbound::WorkItemProvider,
    WorkItemError,
};

use self::conversions::{
    html_contains_images, html_to_markdown, to_domain_comment, to_domain_iteration,
    to_domain_work_item,
};

/// Adapter that wraps an Azure DevOps `RepoClient` to implement the `WorkItemProvider` port.
pub struct AzureDevOpsWorkItemAdapter {
    client: az_devops::RepoClient,
}

impl AzureDevOpsWorkItemAdapter {
    /// Create a new adapter wrapping the given `RepoClient`.
    pub fn new(client: az_devops::RepoClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl WorkItemProvider for AzureDevOpsWorkItemAdapter {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError> {
        let team = format!("{} Team", self.client.project());
        let current_paths: HashSet<String> = match self
            .client
            .get_current_team_iteration_paths(&team)
            .await
        {
            Ok(paths) => paths
                .into_iter()
                .map(|path| normalize_iteration_path(&path))
                .collect(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    team = %team,
                    "Failed to fetch current team iteration paths; falling back to date-based current detection"
                );
                HashSet::new()
            }
        };

        let ado_iterations = self
            .client
            .get_iterations(None)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        Ok(ado_iterations
            .into_iter()
            .map(to_domain_iteration)
            .map(|mut iteration| {
                if current_paths.contains(&iteration.path) {
                    iteration.is_current = true;
                }
                iteration
            })
            .collect())
    }

    async fn query_work_item_ids(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<String>, WorkItemError> {
        // Build WIQL query
        // Classification node paths from the ADO API use the format
        // "\Project\Iteration\Sprint 1", but System.IterationPath on work items
        // uses "Project\Sprint 1" (no leading backslash, no "\Iteration\" segment).
        let query = match iteration_path {
            Some(path) => {
                let path = path.strip_prefix('\\').unwrap_or(path);
                let path = path.replacen("\\Iteration\\", "\\", 1);
                // WIQL uses single-quoted string literals, so escape user-provided single quotes.
                let escaped_path = path.replace('\'', "''");
                format!(
                    "SELECT [System.Id] FROM WorkItems \
                     WHERE [System.TeamProject] = @project \
                     AND [System.IterationPath] UNDER '{}' \
                     ORDER BY [Microsoft.VSTS.Common.Priority] asc",
                    escaped_path
                )
            }
            None => {
                // Use @currentIteration macro to match the port contract
                "SELECT [System.Id] FROM WorkItems \
                 WHERE [System.TeamProject] = @project \
                 AND [System.IterationPath] = @currentIteration \
                 ORDER BY [Microsoft.VSTS.Common.Priority] asc"
                    .to_string()
            }
        };

        // The WIQL API team parameter is required by the SDK but passing an empty
        // string works (the SDK omits it from the URL). Using the project name
        // does NOT work because the default team is typically "{Project} Team".
        let team = team.unwrap_or("");

        tracing::debug!(wiql_query = %query, team = %team, "Executing WIQL query");

        let ids = self
            .client
            .query_work_item_ids_wiql(&query, team)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        Ok(ids.into_iter().map(|id| id.to_string()).collect())
    }

    async fn get_work_items(&self, ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError> {
        let int_ids: Vec<i32> = ids.iter().filter_map(|id| id.parse::<i32>().ok()).collect();

        if int_ids.is_empty() {
            return Ok(vec![]);
        }

        let ado_items = self
            .client
            .get_work_items(int_ids)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        let org = self.client.organization();
        let project = self.client.project();

        Ok(ado_items
            .into_iter()
            .map(|ado| to_domain_work_item(ado, org, project))
            .collect())
    }

    async fn get_board_columns(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Vec<BoardColumn> {
        let result = self.get_board_columns_inner(iteration_path, team).await;

        match result {
            Ok(columns) => columns,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to fetch taskboard column definitions"
                );
                vec![]
            }
        }
    }

    async fn get_taskboard_column_assignments(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> HashMap<String, BoardColumnAssignment> {
        let result = self
            .get_taskboard_column_assignments_inner(iteration_path, team)
            .await;

        match result {
            Ok(map) => map,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to fetch taskboard work item assignments, falling back to state-based mapping"
                );
                HashMap::new()
            }
        }
    }

    async fn get_work_item_comments(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<WorkItemComment>, WorkItemError> {
        let id: i32 = work_item_id.parse().map_err(|_| {
            WorkItemError::ProviderError(format!("Invalid work item ID: {work_item_id}"))
        })?;

        let ado_comments = self
            .client
            .get_work_item_comments(id)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        Ok(ado_comments
            .into_iter()
            .filter(|c| !c.is_deleted)
            .map(to_domain_comment)
            .collect())
    }

    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError> {
        let id: i32 = work_item_id.parse().map_err(|_| {
            WorkItemError::ProviderError(format!("Invalid work item ID: {work_item_id}"))
        })?;

        // Fetch raw ADO work item (need raw HTML for image detection)
        let ado_items = self
            .client
            .get_work_items(vec![id])
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        let ado_item = ado_items.into_iter().next().ok_or_else(|| {
            WorkItemError::ProviderError(format!("Work item {work_item_id} not found"))
        })?;

        // Detect images in raw HTML before conversion
        let mut has_images = false;
        if let Some(ref desc) = ado_item.description {
            if html_contains_images(desc) {
                has_images = true;
            }
        }
        if let Some(ref ac) = ado_item.acceptance_criteria {
            if html_contains_images(ac) {
                has_images = true;
            }
        }

        // Convert to domain model
        let org = self.client.organization();
        let project = self.client.project();
        let mut domain_item = to_domain_work_item(ado_item.clone(), org, project);

        // Batch-fetch titles for parent & related work items
        let ref_ids: Vec<i32> = domain_item
            .parent
            .iter()
            .chain(domain_item.related.iter())
            .filter_map(|r| r.id.parse::<i32>().ok())
            .collect();

        if !ref_ids.is_empty() {
            if let Ok(ref_items) = self.client.get_work_items(ref_ids).await {
                let title_map: HashMap<String, String> = ref_items
                    .into_iter()
                    .map(|wi| (wi.id.to_string(), wi.title))
                    .collect();

                if let Some(ref mut parent) = domain_item.parent {
                    if let Some(title) = title_map.get(&parent.id) {
                        parent.title = Some(title.clone());
                    }
                }
                for rel in &mut domain_item.related {
                    if let Some(title) = title_map.get(&rel.id) {
                        rel.title = Some(title.clone());
                    }
                }
            }
        }

        // Convert HTML fields to Markdown (richer than strip_html)
        let description_md = ado_item.description.as_deref().map(html_to_markdown);
        let acceptance_criteria_md = ado_item
            .acceptance_criteria
            .as_deref()
            .map(html_to_markdown);

        // Fetch comments
        let ado_comments = self
            .client
            .get_work_item_comments(id)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        // Check comment HTML for images too
        for comment in &ado_comments {
            if !comment.is_deleted && html_contains_images(&comment.text) {
                has_images = true;
            }
        }

        let comments: Vec<WorkItemComment> = ado_comments
            .into_iter()
            .filter(|c| !c.is_deleted)
            .map(to_domain_comment)
            .collect();

        // Build markdown document
        let markdown = build_llm_markdown(
            &domain_item,
            description_md,
            acceptance_criteria_md,
            &comments,
            org,
            project,
        );

        Ok((markdown, has_images))
    }

    async fn move_work_item_to_column(
        &self,
        work_item_id: &str,
        target_column_name: &str,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<(), WorkItemError> {
        let work_item_id: i32 = work_item_id.trim().parse().map_err(|_| {
            WorkItemError::InvalidInput(format!("Invalid work item ID: {work_item_id}"))
        })?;

        let target_column_name = target_column_name.trim();
        if target_column_name.is_empty() {
            return Err(WorkItemError::InvalidInput(
                "target column name cannot be empty".to_string(),
            ));
        }

        let team = resolve_team_name(team, self.client.project());
        let iteration = self
            .resolve_taskboard_iteration(iteration_path, &team)
            .await?;
        let columns = self
            .client
            .get_taskboard_columns(&team)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;
        let target_column = columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(target_column_name))
            .ok_or_else(|| {
                WorkItemError::InvalidInput(format!(
                    "Unknown board column '{target_column_name}'"
                ))
            })?;

        self.client
            .move_taskboard_work_item_to_column(
                &team,
                &iteration.id,
                work_item_id,
                &target_column.name,
            )
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))
    }
}

fn normalize_iteration_path(path: &str) -> String {
    let path = path.strip_prefix('\\').unwrap_or(path);
    path.replacen("\\Iteration\\", "\\", 1)
}

/// Build a Markdown document from a work item and its comments, for LLM consumption.
fn build_llm_markdown(
    item: &WorkItem,
    description_md: Option<String>,
    acceptance_criteria_md: Option<String>,
    comments: &[WorkItemComment],
    org: &str,
    project: &str,
) -> String {
    let mut md = String::new();
    // Instructions preamble with org/project context and az CLI examples
    writeln!(
        md,
        "> **Source:** Azure DevOps — org: `{org}`, project: `{project}`"
    )
    .unwrap();
    writeln!(md, ">").unwrap();
    writeln!(
        md,
        "> You can use the `az boards` CLI to fetch more context:"
    )
    .unwrap();
    writeln!(md, "> ```bash").unwrap();
    writeln!(md, "> # Fetch a work item by ID").unwrap();
    writeln!(
        md,
        "> az boards work-item show --id <ID> --org https://dev.azure.com/{org}"
    )
    .unwrap();
    writeln!(md, ">").unwrap();
    writeln!(md, "> # Query work items with WIQL").unwrap();

    // Use the item's actual iteration path when available; fall back to a project-level query.
    if let Some(ref ip) = item.iteration_path {
        writeln!(
            md,
            "> az boards query --wiql \"SELECT [System.Id], [System.Title], [System.State] \
             FROM WorkItems WHERE [System.TeamProject] = '{project}' \
             AND [System.IterationPath] UNDER '{ip}'\" \
             --org https://dev.azure.com/{org}"
        )
        .unwrap();
    } else {
        writeln!(
            md,
            "> az boards query --wiql \"SELECT [System.Id], [System.Title], [System.State] \
             FROM WorkItems WHERE [System.TeamProject] = '{project}'\" \
             --org https://dev.azure.com/{org}"
        )
        .unwrap();
    }

    writeln!(md, "> ```").unwrap();
    writeln!(md).unwrap();

    writeln!(md, "## {} #{}: {}", item.category, item.id, item.title).unwrap();
    writeln!(md).unwrap();

    // Metadata
    let mut meta = vec![format!("**State:** {}", item.state_name)];
    if let Some(p) = item.priority {
        meta.push(format!("**Priority:** {p}"));
    }
    if let Some(ref a) = item.assigned_to {
        meta.push(format!("**Assigned To:** {}", a.display_name));
    }
    writeln!(md, "{}", meta.join(" | ")).unwrap();

    let mut location = Vec::new();
    if let Some(ref ip) = item.iteration_path {
        location.push(format!("**Iteration:** {ip}"));
    }
    if let Some(ref ap) = item.area_path {
        location.push(format!("**Area:** {ap}"));
    }
    if !location.is_empty() {
        writeln!(md, "{}", location.join(" | ")).unwrap();
    }

    if !item.tags.is_empty() {
        writeln!(md, "**Tags:** {}", item.tags.join(", ")).unwrap();
    }

    if let Some(ref desc) = description_md {
        let desc = desc.trim();
        if !desc.is_empty() {
            writeln!(md).unwrap();
            writeln!(md, "### Description").unwrap();
            writeln!(md, "{desc}").unwrap();
        }
    }

    if let Some(ref ac) = acceptance_criteria_md {
        let ac = ac.trim();
        if !ac.is_empty() {
            writeln!(md).unwrap();
            writeln!(md, "### Acceptance Criteria").unwrap();
            writeln!(md, "{ac}").unwrap();
        }
    }

    if let Some(ref parent) = item.parent {
        writeln!(md).unwrap();
        writeln!(md, "### Parent Work Item").unwrap();
        write!(md, "#{}", parent.id).unwrap();
        if let Some(ref title) = parent.title {
            write!(md, " - {title}").unwrap();
        }
        writeln!(md).unwrap();
    }

    if !item.related.is_empty() {
        writeln!(md).unwrap();
        writeln!(md, "### Related Items").unwrap();
        for rel in &item.related {
            write!(md, "- #{}", rel.id).unwrap();
            if let Some(ref title) = rel.title {
                write!(md, " - {title}").unwrap();
            }
            writeln!(md).unwrap();
        }
    }

    if !item.pull_requests.is_empty() {
        writeln!(md).unwrap();
        writeln!(md, "### Linked Pull Requests").unwrap();
        for pr in &item.pull_requests {
            writeln!(md, "- [PR #{}]({})", pr.id, pr.url).unwrap();
        }
    }

    if !comments.is_empty() {
        let format = time::format_description::parse("[year]-[month]-[day] [hour]:[minute]")
            .expect("valid format");
        writeln!(md).unwrap();
        writeln!(md, "### Comments").unwrap();
        for comment in comments {
            let date = comment.created_at.format(&format).unwrap_or_default();
            writeln!(md).unwrap();
            writeln!(md, "**{}** ({date}):", comment.author_name).unwrap();
            writeln!(md, "{}", comment.text.trim()).unwrap();
        }
    }

    md
}

impl AzureDevOpsWorkItemAdapter {
    /// Inner implementation for `get_board_columns` that returns a Result,
    /// allowing the trait method to catch errors and return an empty vector.
    async fn get_board_columns_inner(
        &self,
        _iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<BoardColumn>, WorkItemError> {
        let team = resolve_team_name(team, self.client.project());
        let columns = self
            .client
            .get_taskboard_columns(&team)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        Ok(columns
            .into_iter()
            .map(|column| BoardColumn {
                id: column
                    .id
                    .unwrap_or_else(|| synthetic_column_id_from_name(&column.name)),
                name: column.name,
                order: column.order,
            })
            .collect())
    }

    /// Inner implementation for `get_taskboard_column_assignments` that returns a Result,
    /// allowing the trait method to catch errors and return an empty map.
    async fn get_taskboard_column_assignments_inner(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<HashMap<String, BoardColumnAssignment>, WorkItemError> {
        let team = resolve_team_name(team, self.client.project());
        let iteration = self
            .resolve_taskboard_iteration(iteration_path, &team)
            .await?;

        tracing::debug!(
            iteration_name = %iteration.name,
            iteration_id = %iteration.id,
            team = %team,
            "Looking up taskboard columns"
        );

        let assignments = self
            .client
            .get_taskboard_work_item_columns(&team, &iteration.id)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        Ok(assignments
            .into_iter()
            .map(|(id, assignment)| {
                (
                    id.to_string(),
                    BoardColumnAssignment {
                        column_id: assignment.column_id,
                        column_name: assignment.column_name,
                    },
                )
            })
            .collect())
    }

    async fn resolve_taskboard_iteration(
        &self,
        iteration_path: Option<&str>,
        team: &str,
    ) -> Result<az_devops::TeamIteration, WorkItemError> {
        // Use the work API's team iterations (with GUID IDs) because
        // the taskboard endpoints require GUID iteration IDs.
        let iterations = self
            .client
            .get_team_iterations(team)
            .await
            .map_err(|e| WorkItemError::ProviderError(e.to_string()))?;

        let iteration = match iteration_path {
            Some(path) => {
                let normalized = normalize_iteration_path(path);
                iterations
                    .into_iter()
                    .find(|it| normalize_iteration_path(&it.path) == normalized)
            }
            None => {
                let current_paths: HashSet<String> = self
                    .client
                    .get_current_team_iteration_paths(team)
                    .await
                    .map_err(|e| WorkItemError::ProviderError(e.to_string()))?
                    .into_iter()
                    .map(|path| normalize_iteration_path(&path))
                    .collect();

                iterations.into_iter().find(|it| {
                    let normalized = normalize_iteration_path(&it.path);
                    current_paths.contains(&normalized)
                })
            }
        };

        iteration.ok_or_else(|| match iteration_path {
            Some(path) => WorkItemError::ProviderError(format!(
                "No matching iteration found for taskboard lookup (path: {path})"
            )),
            None => WorkItemError::ProviderError(
                "No current team iteration found for taskboard lookup".into(),
            ),
        })
    }
}

fn resolve_team_name(team: Option<&str>, project: &str) -> String {
    team.map(|t| t.to_string())
        .unwrap_or_else(|| format!("{project} Team"))
}
