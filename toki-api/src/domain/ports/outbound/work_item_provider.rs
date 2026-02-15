use std::collections::HashMap;

use async_trait::async_trait;

use crate::domain::{
    models::{BoardColumn, BoardColumnAssignment, Iteration, WorkItem, WorkItemComment},
    WorkItemError,
};

/// Outbound port for work item provider operations.
///
/// This trait defines the contract that any work item provider
/// (Azure DevOps, or future providers like GitHub Issues) must implement.
///
/// The provider is scoped to a specific organization and project,
/// created per-request by the factory.
#[async_trait]
pub trait WorkItemProvider: Send + Sync + 'static {
    /// Get all iterations (sprints) for the project.
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError>;

    /// Query work item IDs matching the given filters.
    ///
    /// - `iteration_path`: Filter by iteration/sprint path (e.g. "Project\\Sprint 5").
    ///   If `None`, uses the current iteration.
    /// - `team`: Team context for WIQL macros like `@currentIteration`.
    ///   If `None`, defaults to the project name.
    async fn query_work_item_ids(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<String>, WorkItemError>;

    /// Get full work item details for a batch of IDs.
    async fn get_work_items(&self, ids: &[String]) -> Result<Vec<WorkItem>, WorkItemError>;

    /// Get ordered board columns for the current sprint taskboard.
    ///
    /// Returns an empty vector on failure (non-fatal).
    async fn get_board_columns(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Vec<BoardColumn>;

    /// Get sprint taskboard column assignments for work items in an iteration.
    ///
    /// Returns a map of work_item_id → assigned column data.
    /// This reflects the actual taskboard column, which may differ from `System.State`
    /// when the sprint taskboard is customized.
    ///
    /// Returns an empty map on failure (non-fatal).
    async fn get_taskboard_column_assignments(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> HashMap<String, BoardColumnAssignment>;

    /// Get comments on a work item.
    #[allow(dead_code)]
    async fn get_work_item_comments(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<WorkItemComment>, WorkItemError>;

    /// Format a work item with comments as Markdown for LLM consumption.
    ///
    /// Returns `(markdown, has_images)`. The adapter needs access to the raw HTML
    /// to detect images before converting to Markdown, which is why this lives
    /// on the outbound port rather than the inbound service.
    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError>;
}
