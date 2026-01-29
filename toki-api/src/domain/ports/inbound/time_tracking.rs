use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        Activity, ActiveTimer, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId,
        StartTimerRequest, TimeEntry, TimeInfo, TimerId, UserId,
    },
    TimeTrackingError,
};

/// Inbound port for time tracking operations.
///
/// This trait defines the use cases that HTTP handlers can invoke.
/// It orchestrates the outbound ports (clients, repositories) to fulfill requests.
///
/// Note: The service is created per-request with the user's credentials,
/// so user_id is not passed to individual methods.
#[async_trait]
pub trait TimeTrackingService: Send + Sync + 'static {
    /// Get the currently running timer, if any.
    async fn get_active_timer(&self) -> Result<Option<ActiveTimer>, TimeTrackingError>;

    /// Start a new timer.
    async fn start_timer(&self, req: &StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError>;

    /// Stop the currently running timer without saving to Milltime.
    async fn stop_timer(&self) -> Result<(), TimeTrackingError>;

    /// Save/register the current timer as a time entry in Milltime.
    async fn save_timer(&self, note: Option<&str>) -> Result<TimerId, TimeTrackingError>;

    /// Get available projects for time tracking.
    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError>;

    /// Get available activities for a specific project.
    async fn get_activities(
        &self,
        project_id: &ProjectId,
        date_range: (Date, Date),
    ) -> Result<Vec<Activity>, TimeTrackingError>;

    // ========================================================================
    // Calendar/Time Entry Operations
    // ========================================================================

    /// Get time tracking statistics for a date range.
    async fn get_time_info(
        &self,
        date_range: (Date, Date),
    ) -> Result<TimeInfo, TimeTrackingError>;

    /// Get time entries for a date range.
    ///
    /// Merges provider entries with local timer history for start/end times.
    /// If `unique` is true, returns only unique combinations of
    /// project/activity/note.
    async fn get_time_entries(
        &self,
        user_id: &UserId,
        date_range: (Date, Date),
        unique: bool,
    ) -> Result<Vec<TimeEntry>, TimeTrackingError>;

    /// Create a new time entry.
    ///
    /// Creates the entry in the provider and persists to local timer history.
    async fn create_time_entry(
        &self,
        user_id: &UserId,
        request: &CreateTimeEntryRequest,
    ) -> Result<(), TimeTrackingError>;

    /// Edit an existing time entry.
    ///
    /// Updates both the provider and local timer history.
    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<(), TimeTrackingError>;

    /// Delete a time entry.
    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError>;
}
