use async_trait::async_trait;

use crate::domain::{
    models::{BoardData, Iteration},
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

    /// Get board columns and work items, optionally filtered by iteration and team.
    async fn get_board_data(
        &self,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<BoardData, WorkItemError>;

    /// Format a work item with comments as Markdown for LLM consumption.
    ///
    /// Returns `(markdown, has_images)`.
    async fn format_work_item_for_llm(
        &self,
        work_item_id: &str,
    ) -> Result<(String, bool), WorkItemError>;

    /// Move a work item to a target board column.
    async fn move_work_item_to_column(
        &self,
        work_item_id: &str,
        target_column_name: &str,
        iteration_path: Option<&str>,
        team: Option<&str>,
    ) -> Result<(), WorkItemError>;
}
