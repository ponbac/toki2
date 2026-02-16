use time::{Duration, OffsetDateTime, Time};

use crate::domain::models::{
    BoardState, Iteration, PullRequestRef, WorkItem, WorkItemCategory, WorkItemComment,
    WorkItemPerson, WorkItemRef,
};

use super::{normalize_iteration_path, urls::AzureDevOpsUrl};

/// Convert an Azure DevOps work item to a domain work item.
pub fn to_domain_work_item(ado: az_devops::WorkItem, org: &str, project: &str) -> WorkItem {
    let board_state = map_board_state(ado.board_column.as_deref(), &ado.state);
    let board_column_name = ado.board_column.clone();
    let category = map_category(&ado.item_type);

    let assigned_to = ado.assigned_to.map(|identity| WorkItemPerson {
        display_name: identity.display_name,
        unique_name: if identity.unique_name.is_empty() {
            None
        } else {
            Some(identity.unique_name)
        },
        image_url: identity.avatar_url,
    });

    let created_by = ado.created_by.map(|identity| WorkItemPerson {
        display_name: identity.display_name,
        unique_name: if identity.unique_name.is_empty() {
            None
        } else {
            Some(identity.unique_name)
        },
        image_url: identity.avatar_url,
    });

    let description = ado.description.as_deref().map(strip_html);
    let acceptance_criteria = ado.acceptance_criteria.as_deref().map(strip_html);

    let tags = ado
        .tags
        .as_deref()
        .map(|t| {
            t.split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Extract parent from relations where relation_type contains "Hierarchy-Reverse"
    // Note: r.name is the relation type name (e.g. "Parent"), not the work item title.
    let parent = ado
        .relations
        .iter()
        .find(|r| r.relation_type.contains("Hierarchy-Reverse"))
        .map(|r| WorkItemRef {
            id: r
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| extract_id_from_url(&r.url)),
            title: None,
        });

    // Extract related items from relations where relation_type contains
    // "Hierarchy-Forward" (children) or "Related"
    let related: Vec<WorkItemRef> = ado
        .relations
        .iter()
        .filter(|r| {
            r.relation_type.contains("Hierarchy-Forward") || r.relation_type.contains("Related")
        })
        .map(|r| WorkItemRef {
            id: r
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| extract_id_from_url(&r.url)),
            title: None,
        })
        .collect();

    // Extract pull request references from ArtifactLink relations
    let pull_requests: Vec<PullRequestRef> = ado
        .relations
        .iter()
        .filter(|r| r.relation_type == "ArtifactLink")
        .filter_map(|r| parse_pr_artifact_url(&r.url, org))
        .collect();

    let id = ado.id.to_string();
    let url = AzureDevOpsUrl::WorkItem {
        org,
        project,
        id: &id,
    }
    .to_string();

    WorkItem {
        id,
        title: ado.title,
        board_state,
        board_column_id: None,
        board_column_name,
        category,
        state_name: ado.state,
        priority: ado.priority,
        assigned_to,
        created_by,
        description,
        acceptance_criteria,
        iteration_path: ado.iteration_path,
        area_path: ado.area_path,
        tags,
        parent,
        related,
        pull_requests,
        url,
        created_at: ado.created_at,
        changed_at: ado.changed_at,
    }
}

/// Convert an Azure DevOps iteration to a domain iteration.
///
/// The classification node API returns paths like `\Project\Iteration\Sprint 1`,
/// but `System.IterationPath` on work items uses `Project\Sprint 1`.
/// We normalize here so the domain model uses the WIQL-compatible format.
pub fn to_domain_iteration(ado: az_devops::Iteration) -> Iteration {
    to_domain_iteration_at(ado, OffsetDateTime::now_utc())
}

fn to_domain_iteration_at(ado: az_devops::Iteration, now: OffsetDateTime) -> Iteration {
    let start_date = ado.start_date;
    let finish_date = ado.finish_date;
    let is_current = is_iteration_current(start_date, finish_date, now);
    let path = normalize_iteration_path(&ado.path);

    Iteration {
        id: ado.id.to_string(),
        name: ado.name,
        path,
        start_date,
        finish_date,
        is_current,
    }
}

fn effective_finish(finish: OffsetDateTime) -> OffsetDateTime {
    if finish.time() == Time::MIDNIGHT {
        // ADO finish dates are often stored as midnight; treat those as inclusive end-of-day.
        finish + Duration::days(1) - Duration::nanoseconds(1)
    } else {
        finish
    }
}

fn is_iteration_current(
    start_date: Option<OffsetDateTime>,
    finish_date: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> bool {
    match (start_date, finish_date) {
        (Some(start), Some(finish)) => now >= start && now <= effective_finish(finish),
        _ => false,
    }
}

/// Map an Azure DevOps work item to a `BoardState`.
///
/// When a board column is available (from `System.BoardColumn`), it takes
/// precedence over `System.State` since board columns reflect the team's
/// actual workflow (e.g. "Ready for development" is a Todo column even
/// though the underlying state may be "Active").
fn map_board_state(board_column: Option<&str>, state: &str) -> BoardState {
    if let Some(col) = board_column {
        match col {
            "New" | "Proposed" | "To Do" | "Approved" | "Ready for development" => {
                return BoardState::Todo;
            }
            "Done" | "Closed" | "Completed" | "Removed" => {
                return BoardState::Done;
            }
            _ => {}
        }
    }

    match state {
        "New" | "Proposed" | "To Do" | "Approved" => BoardState::Todo,
        "Active" | "Committed" | "In Progress" | "Doing" | "Resolved" => BoardState::InProgress,
        "Done" | "Closed" | "Completed" | "Removed" => BoardState::Done,
        _ => BoardState::Todo,
    }
}

/// Map an Azure DevOps work item type string to a `WorkItemCategory`.
fn map_category(work_item_type: &str) -> WorkItemCategory {
    match work_item_type {
        "User Story" => WorkItemCategory::UserStory,
        "Bug" => WorkItemCategory::Bug,
        "Task" => WorkItemCategory::Task,
        "Feature" => WorkItemCategory::Feature,
        "Epic" => WorkItemCategory::Epic,
        other => WorkItemCategory::Other(other.to_string()),
    }
}

/// Strip HTML tags from a string and decode common HTML entities.
///
/// This is a basic implementation that handles most common cases:
/// - Removes all `<tag>` elements
/// - Decodes `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&nbsp;`
/// - Trims leading/trailing whitespace
fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut inside_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => result.push(ch),
            _ => {}
        }
    }

    // Decode common HTML entities
    let result = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'");

    result.trim().to_string()
}

/// Parse a PR artifact URL into a `PullRequestRef`.
///
/// Artifact URLs look like:
/// `vstfs:///Git/PullRequestId/{projectId}%2F{repoId}%2F{prId}`
fn parse_pr_artifact_url(url: &str, org: &str) -> Option<PullRequestRef> {
    let prefix = "vstfs:///Git/PullRequestId/";
    let payload = url.strip_prefix(prefix)?;
    // Payload is URL-encoded: {projectId}%2F{repoId}%2F{prId}
    // Azure can emit lowercase hex escapes ("%2f"), so accept both.
    let decoded = payload.replace("%2F", "/").replace("%2f", "/");
    let mut parts = decoded.splitn(3, '/');
    let project_id = parts.next()?.to_string();
    let repository_id = parts.next()?.to_string();
    let id = parts.next()?.to_string();
    if project_id.is_empty() || repository_id.is_empty() || id.is_empty() {
        return None;
    }
    let url = AzureDevOpsUrl::PullRequest {
        org,
        project: &project_id,
        repo: &repository_id,
        id: &id,
    }
    .to_string();
    Some(PullRequestRef {
        id,
        repository_id,
        project_id,
        url,
    })
}

/// Extract a work item ID from an Azure DevOps URL.
///
/// URLs typically look like:
/// `https://dev.azure.com/{org}/{project}/_apis/wit/workItems/{id}`
fn extract_id_from_url(url: &str) -> String {
    url.rsplit('/').next().unwrap_or("0").to_string()
}

/// Check if an HTML string contains `<img` tags (case-insensitive).
pub fn html_contains_images(html: &str) -> bool {
    html.to_ascii_lowercase().contains("<img")
}

/// Convert HTML to Markdown using htmd.
pub fn html_to_markdown(html: &str) -> String {
    htmd::convert(html).unwrap_or_else(|_| strip_html(html))
}

/// Convert an Azure DevOps work item comment to a domain comment.
///
/// Converts the HTML text to Markdown.
pub fn to_domain_comment(ado: az_devops::WorkItemComment) -> WorkItemComment {
    WorkItemComment {
        id: ado.id.to_string(),
        text: html_to_markdown(&ado.text),
        author_name: ado.author_name,
        created_at: ado.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn test_strip_html_entities() {
        assert_eq!(
            strip_html("&lt;script&gt; &amp; &quot;test&quot;"),
            "<script> & \"test\""
        );
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn test_strip_html_no_tags() {
        assert_eq!(strip_html("plain text"), "plain text");
    }

    #[test]
    fn test_strip_html_nbsp() {
        assert_eq!(strip_html("hello&nbsp;world"), "hello world");
    }

    #[test]
    fn test_map_board_state_from_state() {
        assert_eq!(map_board_state(None, "New"), BoardState::Todo);
        assert_eq!(map_board_state(None, "Proposed"), BoardState::Todo);
        assert_eq!(map_board_state(None, "To Do"), BoardState::Todo);
        assert_eq!(map_board_state(None, "Approved"), BoardState::Todo);
        assert_eq!(map_board_state(None, "Active"), BoardState::InProgress);
        assert_eq!(map_board_state(None, "Committed"), BoardState::InProgress);
        assert_eq!(map_board_state(None, "In Progress"), BoardState::InProgress);
        assert_eq!(map_board_state(None, "Doing"), BoardState::InProgress);
        assert_eq!(map_board_state(None, "Resolved"), BoardState::InProgress);
        assert_eq!(map_board_state(None, "Done"), BoardState::Done);
        assert_eq!(map_board_state(None, "Closed"), BoardState::Done);
        assert_eq!(map_board_state(None, "Completed"), BoardState::Done);
        assert_eq!(map_board_state(None, "Removed"), BoardState::Done);
        assert_eq!(map_board_state(None, "SomeCustomState"), BoardState::Todo);
    }

    #[test]
    fn test_map_board_state_board_column_overrides_state() {
        // "Ready for development" board column should map to Todo even though state is "Active"
        assert_eq!(
            map_board_state(Some("Ready for development"), "Active"),
            BoardState::Todo
        );
        // Unknown board column falls through to state-based mapping
        assert_eq!(
            map_board_state(Some("In review"), "Active"),
            BoardState::InProgress
        );
    }

    #[test]
    fn test_map_category() {
        assert_eq!(map_category("User Story"), WorkItemCategory::UserStory);
        assert_eq!(map_category("Bug"), WorkItemCategory::Bug);
        assert_eq!(map_category("Task"), WorkItemCategory::Task);
        assert_eq!(map_category("Feature"), WorkItemCategory::Feature);
        assert_eq!(map_category("Epic"), WorkItemCategory::Epic);
        assert_eq!(
            map_category("Issue"),
            WorkItemCategory::Other("Issue".to_string())
        );
    }

    #[test]
    fn test_extract_id_from_url() {
        assert_eq!(
            extract_id_from_url("https://dev.azure.com/myorg/myproject/_apis/wit/workItems/12345"),
            "12345"
        );
    }

    #[test]
    fn test_parse_pr_artifact_url_valid() {
        let url = "vstfs:///Git/PullRequestId/abc-project%2Frepo-123%2F42";
        let pr = parse_pr_artifact_url(url, "myorg").unwrap();
        assert_eq!(pr.project_id, "abc-project");
        assert_eq!(pr.repository_id, "repo-123");
        assert_eq!(pr.id, "42");
        assert_eq!(
            pr.url,
            "https://dev.azure.com/myorg/abc-project/_git/repo-123/pullrequest/42"
        );
    }

    #[test]
    fn test_parse_pr_artifact_url_valid_with_lowercase_encoding() {
        let url = "vstfs:///Git/PullRequestId/abc-project%2frepo-123%2f42";
        let pr = parse_pr_artifact_url(url, "myorg").unwrap();
        assert_eq!(pr.project_id, "abc-project");
        assert_eq!(pr.repository_id, "repo-123");
        assert_eq!(pr.id, "42");
        assert_eq!(
            pr.url,
            "https://dev.azure.com/myorg/abc-project/_git/repo-123/pullrequest/42"
        );
    }

    #[test]
    fn test_parse_pr_artifact_url_with_guids() {
        let url = "vstfs:///Git/PullRequestId/d4e5f6a7-b8c9-0123-4567-89abcdef0123%2Fa1b2c3d4-e5f6-7890-abcd-ef0123456789%2F999";
        let pr = parse_pr_artifact_url(url, "myorg").unwrap();
        assert_eq!(pr.project_id, "d4e5f6a7-b8c9-0123-4567-89abcdef0123");
        assert_eq!(pr.repository_id, "a1b2c3d4-e5f6-7890-abcd-ef0123456789");
        assert_eq!(pr.id, "999");
        assert_eq!(
            pr.url,
            "https://dev.azure.com/myorg/d4e5f6a7-b8c9-0123-4567-89abcdef0123/_git/a1b2c3d4-e5f6-7890-abcd-ef0123456789/pullrequest/999"
        );
    }

    #[test]
    fn test_parse_pr_artifact_url_invalid_prefix() {
        assert!(parse_pr_artifact_url("vstfs:///Git/CommitId/abc%2Fdef%2F1", "myorg").is_none());
    }

    #[test]
    fn test_parse_pr_artifact_url_missing_parts() {
        assert!(parse_pr_artifact_url("vstfs:///Git/PullRequestId/abc%2Fdef", "myorg").is_none());
    }

    #[test]
    fn test_to_domain_iteration_treats_midnight_finish_as_end_of_day() {
        let iteration = az_devops::Iteration {
            id: 123,
            name: "Sprint 35".to_string(),
            path: "\\MyProject\\Iteration\\Sprint 35".to_string(),
            start_date: Some(
                OffsetDateTime::parse(
                    "2026-02-02T00:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap(),
            ),
            finish_date: Some(
                OffsetDateTime::parse(
                    "2026-02-15T00:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap(),
            ),
        };

        let now = OffsetDateTime::parse(
            "2026-02-15T12:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();

        let domain = to_domain_iteration_at(iteration, now);
        assert!(domain.is_current);
    }

    #[test]
    fn test_to_domain_iteration_is_not_current_after_end_of_finish_day() {
        let iteration = az_devops::Iteration {
            id: 124,
            name: "Sprint 35".to_string(),
            path: "\\MyProject\\Iteration\\Sprint 35".to_string(),
            start_date: Some(
                OffsetDateTime::parse(
                    "2026-02-02T00:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap(),
            ),
            finish_date: Some(
                OffsetDateTime::parse(
                    "2026-02-15T00:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .unwrap(),
            ),
        };

        let now = OffsetDateTime::parse(
            "2026-02-16T00:00:00Z",
            &time::format_description::well_known::Rfc3339,
        )
        .unwrap();

        let domain = to_domain_iteration_at(iteration, now);
        assert!(!domain.is_current);
    }
}
