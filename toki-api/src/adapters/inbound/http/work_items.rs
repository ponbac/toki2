//! HTTP adapter for work item board operations.
//!
//! Defines the factory trait for creating WorkItemService instances from HTTP requests.
//! The concrete implementation lives in `crate::factory` (the composition root).

use async_trait::async_trait;
use axum::http::StatusCode;

use crate::domain::{models::WorkItemProject, ports::inbound::WorkItemService};

/// Error returned when creating a WorkItemService or fetching projects fails.
#[derive(Debug)]
pub struct WorkItemServiceError {
    pub status: StatusCode,
    pub message: String,
}

/// Factory trait for creating WorkItemService instances scoped to an organization and project.
///
/// This trait lives in the inbound adapter because it bridges HTTP concerns (StatusCode)
/// with domain service creation. The concrete implementation lives in `crate::factory`
/// where it's allowed to know about concrete outbound adapters (AzureDevOpsWorkItemAdapter, etc.).
///
/// Key differences from TimeTrackingServiceFactory:
/// - Takes `organization` + `project` (not `CookieJar`) — ADO uses PAT auth from repo_clients, not cookies.
/// - `get_available_projects()` lives on the factory (cross-project concern), not the service (project-scoped).
#[async_trait]
pub trait WorkItemServiceFactory: Send + Sync + 'static {
    /// Create a WorkItemService scoped to a specific organization and project.
    async fn create_service(
        &self,
        organization: &str,
        project: &str,
    ) -> Result<Box<dyn WorkItemService>, WorkItemServiceError>;

    /// Get all projects the user has access to (cross-project).
    async fn get_available_projects(
        &self,
        user_id: i32,
    ) -> Result<Vec<WorkItemProject>, WorkItemServiceError>;
}
