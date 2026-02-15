use async_trait::async_trait;

use crate::domain::{
    models::{Iteration, WorkItem},
    WorkItemError,
};

/// Inbound port for work item board operations.
///
/// This trait defines the use cases that HTTP handlers can invoke.
/// It orchestrates the outbound port (WorkItemProvider) to fulfill requests.
///
/// The service is created per-request, scoped to a specific organization and project.
#[async_trait]
pub trait WorkItemService: Send + Sync + 'static {
    /// Get all iterations (sprints) for the project.
    async fn get_iterations(&self) -> Result<Vec<Iteration>, WorkItemError>;

    /// Get work items for the board, optionally filtered by iteration and team.
    ///
    /// Returns items sorted by board state, then by priority.
    async fn get_board_items(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<Vec<WorkItem>, WorkItemError>;

    /// Format a work item with comments as Markdown for LLM consumption.
    ///
    /// Returns `(markdown, has_images)`.
    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError>;
}
