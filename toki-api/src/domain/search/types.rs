//! Core types for the search domain.

use serde::{Deserialize, Serialize};
use sqlx::Type;
use time::OffsetDateTime;

/// Source type for searchable documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[sqlx(type_name = "search_source", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    Pr,
    WorkItem,
}

impl std::fmt::Display for SearchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchSource::Pr => write!(f, "pr"),
            SearchSource::WorkItem => write!(f, "work_item"),
        }
    }
}

/// A document to be indexed for search.
#[derive(Debug, Clone)]
pub struct SearchDocument {
    /// Source type (PR or work item)
    pub source_type: SearchSource,
    /// Unique identifier: "org/project/repo/123" for PR, "org/project/123" for WI
    pub source_id: String,
    /// PR number or work item ID
    pub external_id: i32,
    /// Document title
    pub title: String,
    /// Document description/body
    pub description: Option<String>,
    /// Combined searchable content (description + comments + commits)
    pub content: Option<String>,
    /// Azure DevOps organization
    pub organization: String,
    /// Azure DevOps project
    pub project: String,
    /// Repository name (None for work items)
    pub repo_name: Option<String>,
    /// Status: 'active', 'completed', 'abandoned' for PRs; 'New', 'Active', 'Closed' for WI
    pub status: String,
    /// Author user ID
    pub author_id: Option<String>,
    /// Author display name
    pub author_name: Option<String>,
    /// Assigned user ID
    pub assigned_to_id: Option<String>,
    /// Assigned user display name
    pub assigned_to_name: Option<String>,
    /// Priority (1-4 for work items, None for PRs)
    pub priority: Option<i32>,
    /// Item type: 'Bug', 'Task', 'User Story' for WI; None for PRs
    pub item_type: Option<String>,
    /// Whether PR is a draft
    pub is_draft: bool,
    /// Creation timestamp
    pub created_at: OffsetDateTime,
    /// Last update timestamp
    pub updated_at: OffsetDateTime,
    /// Closed/completed timestamp
    pub closed_at: Option<OffsetDateTime>,
    /// Direct URL to the item
    pub url: String,
    /// Parent work item ID (for hierarchical items)
    pub parent_id: Option<i32>,
    /// Work items linked to this PR
    pub linked_work_items: Vec<i32>,
    /// Pre-computed embedding vector (1536 dimensions for Gemini)
    pub embedding: Option<Vec<f32>>,
}

/// Result from a search query.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: i32,
    pub source_type: SearchSource,
    pub source_id: String,
    pub external_id: i32,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: Option<i32>,
    pub item_type: Option<String>,
    pub author_name: Option<String>,
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    /// Combined relevance score (higher is better)
    pub score: f64,
}

/// Parsed search query with extracted filters.
#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    /// Remaining search text after filter extraction
    pub search_text: String,
    /// Extracted filters
    pub filters: SearchFilters,
}

/// Filters extracted from a search query.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Filter by source type
    pub source_type: Option<SearchSource>,
    /// Filter by organization
    pub organization: Option<String>,
    /// Filter by project
    pub project: Option<String>,
    /// Filter by repository name
    pub repo_name: Option<String>,
    /// Filter by status (multiple allowed)
    pub status: Option<Vec<String>>,
    /// Filter by priority (multiple allowed)
    pub priority: Option<Vec<i32>>,
    /// Filter by item type (multiple allowed)
    pub item_type: Option<Vec<String>>,
    /// Filter by author name/ID
    pub author: Option<String>,
    /// Filter by assignee name/ID
    pub assigned_to: Option<String>,
    /// Filter for draft PRs only
    pub is_draft: Option<bool>,
    /// Filter: created after this date
    pub created_after: Option<OffsetDateTime>,
    /// Filter: created before this date
    pub created_before: Option<OffsetDateTime>,
    /// Filter: updated after this date
    pub updated_after: Option<OffsetDateTime>,
}

/// Statistics from a sync operation.
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub prs_indexed: usize,
    pub work_items_indexed: usize,
    pub documents_deleted: usize,
    pub errors: usize,
}

impl SyncStats {
    #[allow(dead_code)]
    pub fn total_indexed(&self) -> usize {
        self.prs_indexed + self.work_items_indexed
    }
}

/// Intermediate type for PR data from ADO.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PullRequestDocument {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    pub status: String,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub is_draft: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub closed_at: Option<OffsetDateTime>,
    pub url: String,
    /// Combined text from commits and comments
    pub additional_content: String,
    /// Linked work item IDs
    pub linked_work_items: Vec<i32>,
}

/// Intermediate type for work item data from ADO.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkItemDocument {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub organization: String,
    pub project: String,
    pub status: String,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub assigned_to_id: Option<String>,
    pub assigned_to_name: Option<String>,
    pub priority: Option<i32>,
    pub item_type: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub closed_at: Option<OffsetDateTime>,
    pub url: String,
    pub parent_id: Option<i32>,
    /// Combined text from comments
    pub additional_content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_source_display() {
        assert_eq!(SearchSource::Pr.to_string(), "pr");
        assert_eq!(SearchSource::WorkItem.to_string(), "work_item");
    }

    #[test]
    fn sync_stats_total() {
        let stats = SyncStats {
            prs_indexed: 10,
            work_items_indexed: 20,
            ..Default::default()
        };
        assert_eq!(stats.total_indexed(), 30);
    }
}
