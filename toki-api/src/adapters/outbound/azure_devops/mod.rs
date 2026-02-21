mod conversions;
mod urls;

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use async_trait::async_trait;
use az_devops::RepoClientError;
use tokio::sync::OnceCell;
use url::Url;

use crate::domain::{
    models::{
        synthetic_column_id_from_name, BoardColumn, BoardColumnAssignment, Iteration, WorkItem,
        WorkItemComment, WorkItemImage,
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
    api_base_url: Url,
    resolved_default_team: OnceCell<String>,
}

const MAX_WORK_ITEM_IMAGE_BYTES: usize = 5 * 1024 * 1024;

impl AzureDevOpsWorkItemAdapter {
    /// Create a new adapter wrapping the given `RepoClient`.
    pub fn new(client: az_devops::RepoClient, api_base_url: Url) -> Self {
        Self {
            client,
            api_base_url,
            resolved_default_team: OnceCell::new(),
        }
    }

    async fn current_iteration_paths_for_default_team(&self) -> HashSet<String> {
        let default_team = match self.resolve_default_team(None).await {
            Ok(team) => team,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to resolve default team for iterations; falling back to date-based current detection"
                );
                return HashSet::new();
            }
        };

        match self
            .client
            .get_current_team_iteration_paths(&default_team)
            .await
        {
            Ok(paths) => paths
                .into_iter()
                .map(|path| normalize_iteration_path(&path))
                .collect(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    team = %default_team,
                    "Failed to fetch current team iteration paths; falling back to date-based current detection"
                );
                HashSet::new()
            }
        }
    }
}

#[async_trait]
impl WorkItemProvider for AzureDevOpsWorkItemAdapter {
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError> {
        let current_paths = self.current_iteration_paths_for_default_team().await;

        let ado_iterations = self
            .client
            .get_iterations(None)
            .await
            .map_err(to_provider_error)?;

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

        // For explicit iteration paths, use project-scope WIQL and avoid
        // team-scoped WIQL routes.
        if iteration_path.is_some() {
            tracing::debug!(wiql_query = %query, "Executing project-scope WIQL query");
            let ids = self
                .client
                .query_work_item_ids_wiql_project_scope(&query)
                .await
                .map_err(to_provider_error)?;
            return Ok(format_work_item_ids(ids));
        }

        let resolved_team = self.resolve_default_team(team).await?;
        tracing::debug!(
            wiql_query = %query,
            team = %resolved_team,
            "Executing team-scoped WIQL query"
        );
        let ids = self
            .client
            .query_work_item_ids_wiql(&query, &resolved_team)
            .await
            .map_err(to_provider_error)?;
        Ok(format_work_item_ids(ids))
    }

    async fn get_work_items(&self, ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError> {
        let int_ids: Vec<i32> = ids
            .iter()
            .filter_map(|id| match id.parse::<i32>() {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::warn!(
                        work_item_id = %id,
                        error = %error,
                        "Skipping non-numeric work item ID"
                    );
                    None
                }
            })
            .collect();

        if int_ids.is_empty() {
            return Ok(vec![]);
        }

        let ado_items = self
            .client
            .get_work_items(int_ids)
            .await
            .map_err(to_provider_error)?;

        let org = self.client.organization();
        let project = self.client.project();

        Ok(ado_items
            .into_iter()
            .map(|ado| to_domain_work_item(ado, org, project, &self.api_base_url))
            .collect())
    }

    async fn get_board_columns(
        &self,
        _iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Vec<BoardColumn> {
        let result = self.get_board_columns_inner(team).await;

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
            WorkItemError::InvalidInput(format!("Invalid work item ID: {work_item_id}"))
        })?;

        let ado_comments = self
            .client
            .get_work_item_comments(id)
            .await
            .map_err(to_provider_error)?;

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
            WorkItemError::InvalidInput(format!("Invalid work item ID: {work_item_id}"))
        })?;

        // Fetch raw ADO work item (need raw HTML for image detection)
        let ado_items = self
            .client
            .get_work_items(vec![id])
            .await
            .map_err(to_provider_error)?;

        let ado_item = ado_items.into_iter().next().ok_or_else(|| {
            WorkItemError::ProviderError(format!("Work item {work_item_id} not found"))
        })?;
        let raw_description = ado_item.description.clone();
        let raw_repro_steps = ado_item.repro_steps.clone();
        let raw_acceptance_criteria = ado_item.acceptance_criteria.clone();

        // Detect images in raw HTML before conversion
        let mut has_images = contains_images([
            raw_description.as_deref(),
            raw_repro_steps.as_deref(),
            raw_acceptance_criteria.as_deref(),
        ]);

        // Convert to domain model
        let org = self.client.organization();
        let project = self.client.project();
        let mut domain_item = to_domain_work_item(ado_item, org, project, &self.api_base_url);

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
        let description_md = raw_description.as_deref().map(html_to_markdown);
        let repro_steps_md = raw_repro_steps.as_deref().map(html_to_markdown);
        let acceptance_criteria_md = raw_acceptance_criteria.as_deref().map(html_to_markdown);

        // Fetch comments
        let ado_comments = self
            .client
            .get_work_item_comments(id)
            .await
            .map_err(to_provider_error)?;

        // Check comment HTML for images too
        has_images = has_images
            || ado_comments
                .iter()
                .any(|comment| !comment.is_deleted && html_contains_images(&comment.text));

        let comments: Vec<WorkItemComment> = ado_comments
            .into_iter()
            .filter(|c| !c.is_deleted)
            .map(to_domain_comment)
            .collect();

        // Build markdown document
        let markdown = build_llm_markdown(
            &domain_item,
            description_md,
            repro_steps_md,
            acceptance_criteria_md,
            &comments,
            org,
            project,
        );

        Ok((markdown, has_images))
    }

    async fn fetch_image(&self, image_url: &str) -> Result<WorkItemImage, WorkItemError> {
        let ParsedAttachmentUrl {
            attachment_id,
            file_name,
        } = parse_ado_attachment_url(image_url, self.client.organization())?;

        let (bytes, content_type) = self
            .client
            .get_work_item_attachment(
                &attachment_id,
                file_name.as_deref(),
                Some(MAX_WORK_ITEM_IMAGE_BYTES),
            )
            .await
            .map_err(map_attachment_error)?;

        if bytes.is_empty() {
            return Err(WorkItemError::ProviderError(
                "Attachment payload was empty".to_string(),
            ));
        }

        if bytes.len() > MAX_WORK_ITEM_IMAGE_BYTES {
            return Err(WorkItemError::InvalidInput(format!(
                "Image payload exceeds {} bytes",
                MAX_WORK_ITEM_IMAGE_BYTES
            )));
        }

        let resolved_content_type = resolve_image_content_type(content_type.as_deref(), &bytes)
            .ok_or_else(|| {
                WorkItemError::InvalidInput("Attachment is not a supported image".to_string())
            })?;

        Ok(WorkItemImage {
            bytes,
            content_type: Some(resolved_content_type),
        })
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

        let resolved_team = self.resolve_default_team(team).await?;
        let iteration = self
            .resolve_taskboard_iteration(iteration_path, &resolved_team)
            .await?;
        let columns = self
            .client
            .get_taskboard_columns(&resolved_team)
            .await
            .map_err(to_provider_error)?;

        let Some(target_column) = columns
            .iter()
            .find(|column| column.name.eq_ignore_ascii_case(target_column_name))
        else {
            return Err(WorkItemError::InvalidInput(format!(
                "Unknown board column '{target_column_name}'"
            )));
        };

        self.client
            .move_taskboard_work_item_to_column(
                &resolved_team,
                &iteration.id,
                work_item_id,
                &target_column.name,
            )
            .await
            .map_err(to_provider_error)
    }
}

fn normalize_iteration_path(path: &str) -> String {
    let path = path.strip_prefix('\\').unwrap_or(path);
    path.replacen("\\Iteration\\", "\\", 1)
}

fn map_attachment_error(error: RepoClientError) -> WorkItemError {
    match error {
        RepoClientError::PayloadTooLarge { max_bytes, .. } => {
            WorkItemError::InvalidInput(format!("Image payload exceeds {max_bytes} bytes"))
        }
        other => to_provider_error(other),
    }
}

#[derive(Debug)]
struct ParsedAttachmentUrl {
    attachment_id: String,
    file_name: Option<String>,
}

fn parse_ado_attachment_url(
    image_url: &str,
    expected_organization: &str,
) -> Result<ParsedAttachmentUrl, WorkItemError> {
    let parsed = Url::parse(image_url)
        .map_err(|_| WorkItemError::InvalidInput("Invalid image URL".to_string()))?;

    if parsed.scheme() != "https" {
        return Err(WorkItemError::InvalidInput(
            "Image URL must use HTTPS".to_string(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL is missing host".to_string()))?;
    if !host.eq_ignore_ascii_case("dev.azure.com") {
        return Err(WorkItemError::InvalidInput(
            "Image URL must target dev.azure.com".to_string(),
        ));
    }

    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL path is invalid".to_string()))?;

    let organization = segments.next().ok_or_else(|| {
        WorkItemError::InvalidInput("Image URL organization is missing".to_string())
    })?;
    if !organization.eq_ignore_ascii_case(expected_organization) {
        return Err(WorkItemError::InvalidInput(
            "Image URL organization does not match board organization".to_string(),
        ));
    }

    // Project segment (name or GUID) is required but not used in downstream lookup.
    let _project = segments
        .next()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL project is missing".to_string()))?;
    let api = segments
        .next()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL path is invalid".to_string()))?;
    let wit = segments
        .next()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL path is invalid".to_string()))?;
    let attachments = segments
        .next()
        .ok_or_else(|| WorkItemError::InvalidInput("Image URL path is invalid".to_string()))?;
    let attachment_id = segments
        .next()
        .ok_or_else(|| WorkItemError::InvalidInput("Image attachment ID is missing".to_string()))?;

    if !api.eq_ignore_ascii_case("_apis")
        || !wit.eq_ignore_ascii_case("wit")
        || !attachments.eq_ignore_ascii_case("attachments")
        || attachment_id.is_empty()
    {
        return Err(WorkItemError::InvalidInput(
            "Image URL is not a work item attachment URL".to_string(),
        ));
    }

    if segments.any(|segment| !segment.is_empty()) {
        return Err(WorkItemError::InvalidInput(
            "Image URL has unexpected path segments".to_string(),
        ));
    }

    let file_name = parsed
        .query_pairs()
        .find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("filename") {
                Some(value.into_owned())
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty());

    Ok(ParsedAttachmentUrl {
        attachment_id: attachment_id.to_string(),
        file_name,
    })
}

pub(super) fn is_allowed_ado_attachment_url(image_url: &str, expected_organization: &str) -> bool {
    parse_ado_attachment_url(image_url, expected_organization).is_ok()
}

fn resolve_image_content_type(content_type: Option<&str>, bytes: &[u8]) -> Option<String> {
    let normalized = content_type
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .to_ascii_lowercase()
        })
        .filter(|value| !value.is_empty());

    if let Some(ref mime) = normalized {
        if mime.starts_with("image/") {
            return Some(mime.clone());
        }
    }

    infer_image_content_type(bytes).map(str::to_string)
}

fn infer_image_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.len() >= 3 && bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 6 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.len() >= 2 && bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }

    None
}

fn escape_wiql_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn contains_images<'a>(fragments: impl IntoIterator<Item = Option<&'a str>>) -> bool {
    fragments.into_iter().flatten().any(html_contains_images)
}

fn append_markdown_section(md: &mut String, title: &str, body: Option<&str>) {
    let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) else {
        return;
    };

    writeln!(md).unwrap();
    writeln!(md, "### {title}").unwrap();
    writeln!(md, "{body}").unwrap();
}

/// Build a Markdown document from a work item and its comments, for LLM consumption.
fn build_llm_markdown(
    item: &WorkItem,
    description_md: Option<String>,
    repro_steps_md: Option<String>,
    acceptance_criteria_md: Option<String>,
    comments: &[WorkItemComment],
    org: &str,
    project: &str,
) -> String {
    let mut md = String::new();
    let escaped_project_for_wiql = escape_wiql_single_quoted(project);

    // Instructions preamble with org/project context and az CLI examples
    writeln!(
        md,
        "> **Source:** Azure DevOps — org: `{org}`, project: `{project}`"
    )
    .unwrap();
    writeln!(md, ">").unwrap();
    writeln!(
        md,
        "> Respond in English. The work item details and comments may be in Swedish; use them as source context but keep your final response in English."
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
        let escaped_iteration_path_for_wiql = escape_wiql_single_quoted(ip);
        writeln!(
            md,
            "> az boards query --wiql \"SELECT [System.Id], [System.Title], [System.State] \
             FROM WorkItems WHERE [System.TeamProject] = '{escaped_project_for_wiql}' \
             AND [System.IterationPath] UNDER '{escaped_iteration_path_for_wiql}'\" \
             --org https://dev.azure.com/{org}"
        )
        .unwrap();
    } else {
        writeln!(
            md,
            "> az boards query --wiql \"SELECT [System.Id], [System.Title], [System.State] \
             FROM WorkItems WHERE [System.TeamProject] = '{escaped_project_for_wiql}'\" \
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

    append_markdown_section(&mut md, "Description", description_md.as_deref());
    append_markdown_section(
        &mut md,
        "Acceptance Criteria",
        acceptance_criteria_md.as_deref(),
    );
    append_markdown_section(&mut md, "Repro Steps", repro_steps_md.as_deref());

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
        team: Option<&str>,
    ) -> Result<Vec<BoardColumn>, WorkItemError> {
        let resolved_team = self.resolve_default_team(team).await?;
        let columns = self
            .client
            .get_taskboard_columns(&resolved_team)
            .await
            .map_err(to_provider_error)?;

        Ok(map_taskboard_columns(columns))
    }

    /// Inner implementation for `get_taskboard_column_assignments` that returns a Result,
    /// allowing the trait method to catch errors and return an empty map.
    async fn get_taskboard_column_assignments_inner(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<HashMap<String, BoardColumnAssignment>, WorkItemError> {
        let resolved_team = self.resolve_default_team(team).await?;
        let iteration = self
            .resolve_taskboard_iteration(iteration_path, &resolved_team)
            .await?;

        tracing::debug!(
            iteration_name = %iteration.name,
            iteration_id = %iteration.id,
            team = %resolved_team,
            "Looking up taskboard columns"
        );

        let assignments = self
            .client
            .get_taskboard_work_item_columns(&resolved_team, &iteration.id)
            .await
            .map_err(to_provider_error)?;

        Ok(map_taskboard_assignments(assignments))
    }

    async fn resolve_default_team(&self, team: Option<&str>) -> Result<String, WorkItemError> {
        if let Some(explicit_team) = team.map(str::trim).filter(|team| !team.is_empty()) {
            return Ok(explicit_team.to_string());
        }

        let selected_team = self
            .resolved_default_team
            .get_or_try_init(|| async {
                let project = self.client.project();
                let project_teams = self
                    .client
                    .get_project_team_names()
                    .await
                    .map_err(to_provider_error)?;
                let selected_team =
                    select_default_project_team(project, self.client.repo_name(), &project_teams)
                    .ok_or_else(|| {
                        WorkItemError::ProviderError(format!(
                            "No teams were found for project '{project}'"
                        ))
                    })?;

                tracing::debug!(
                    project = %project,
                    team = %selected_team,
                    team_count = project_teams.len(),
                    "Resolved default taskboard team"
                );

                Ok(selected_team)
            })
            .await?;

        Ok(selected_team.clone())
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
            .map_err(to_provider_error)?;

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
                    .map_err(to_provider_error)?
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

fn select_default_project_team(
    project: &str,
    repo_name: &str,
    project_teams: &[String],
) -> Option<String> {
    let find_case_insensitive = |needle: &str| {
        project_teams
            .iter()
            .find(|team| team.eq_ignore_ascii_case(needle))
            .cloned()
    };

    find_case_insensitive(&format!("{project} Team"))
        .or_else(|| find_case_insensitive(repo_name))
        .or_else(|| find_case_insensitive(&format!("{repo_name} Team")))
        .or_else(|| project_teams.first().cloned())
}

fn to_provider_error(error: impl ToString) -> WorkItemError {
    WorkItemError::ProviderError(error.to_string())
}

fn map_taskboard_columns(columns: Vec<az_devops::TaskboardColumnDefinition>) -> Vec<BoardColumn> {
    columns
        .into_iter()
        .map(|column| BoardColumn {
            id: column
                .id
                .unwrap_or_else(|| synthetic_column_id_from_name(&column.name)),
            name: column.name,
            order: column.order,
        })
        .collect()
}

fn map_taskboard_assignments(
    assignments: HashMap<i32, az_devops::TaskboardWorkItemColumnAssignment>,
) -> HashMap<String, BoardColumnAssignment> {
    assignments
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
        .collect()
}

fn format_work_item_ids(ids: Vec<i32>) -> Vec<String> {
    ids.into_iter().map(|id| id.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_ado_attachment_url, resolve_image_content_type, select_default_project_team,
        ParsedAttachmentUrl, MAX_WORK_ITEM_IMAGE_BYTES,
    };
    use crate::domain::WorkItemError;

    #[test]
    fn parse_ado_attachment_url_accepts_valid_url() {
        let parsed = parse_ado_attachment_url(
            "https://dev.azure.com/example-org/project-guid/_apis/wit/attachments/683f8fad-56a3-43a0-b268-9ffd026dde6e?fileName=image.png",
            "example-org",
        )
        .expect("expected valid attachment url");

        assert_eq!(parsed.attachment_id, "683f8fad-56a3-43a0-b268-9ffd026dde6e");
        assert_eq!(parsed.file_name.as_deref(), Some("image.png"));
    }

    #[test]
    fn parse_ado_attachment_url_rejects_wrong_host() {
        let err = parse_ado_attachment_url(
            "https://example.com/example-org/project/_apis/wit/attachments/abc",
            "example-org",
        )
        .unwrap_err();

        assert!(matches!(err, WorkItemError::InvalidInput(_)));
    }

    #[test]
    fn parse_ado_attachment_url_rejects_wrong_org() {
        let err = parse_ado_attachment_url(
            "https://dev.azure.com/another-org/project/_apis/wit/attachments/abc",
            "example-org",
        )
        .unwrap_err();

        assert!(matches!(err, WorkItemError::InvalidInput(_)));
    }

    #[test]
    fn resolve_image_content_type_prefers_valid_image_header() {
        let resolved = resolve_image_content_type(Some("image/png; charset=utf-8"), b"test");
        assert_eq!(resolved.as_deref(), Some("image/png"));
    }

    #[test]
    fn resolve_image_content_type_infers_png_when_header_missing() {
        let bytes = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let resolved = resolve_image_content_type(None, &bytes);
        assert_eq!(resolved.as_deref(), Some("image/png"));
    }

    #[test]
    fn max_work_item_image_bytes_is_reasonable() {
        assert_eq!(MAX_WORK_ITEM_IMAGE_BYTES, 5 * 1024 * 1024);
    }

    #[test]
    fn parsed_attachment_url_struct_holds_fields() {
        let parsed = ParsedAttachmentUrl {
            attachment_id: "abc".to_string(),
            file_name: Some("image.png".to_string()),
        };

        assert_eq!(parsed.attachment_id, "abc");
        assert_eq!(parsed.file_name.as_deref(), Some("image.png"));
    }

    #[test]
    fn select_default_project_team_prefers_project_team() {
        let teams = vec![
            "Platform Team".to_string(),
            "Space Ninjas Team".to_string(),
            "Ops".to_string(),
        ];

        let selected = select_default_project_team("Space Ninjas", "platform", &teams);
        assert_eq!(selected.as_deref(), Some("Space Ninjas Team"));
    }

    #[test]
    fn select_default_project_team_prefers_repo_name_when_project_team_missing() {
        let teams = vec![
            "Backend Team".to_string(),
            "Hexagon".to_string(),
            "Frontend Team".to_string(),
        ];

        let selected = select_default_project_team("Quote Manager", "hexagon", &teams);
        assert_eq!(selected.as_deref(), Some("Hexagon"));
    }

    #[test]
    fn select_default_project_team_falls_back_to_first_team() {
        let teams = vec!["Platform Team".to_string(), "Ops".to_string()];
        let selected = select_default_project_team("Space Ninjas", "my-repo", &teams);
        assert_eq!(selected.as_deref(), Some("Platform Team"));
    }
}
