use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        Activity, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId, TimeEntry,
        TimeInfo, TimerId,
    },
    TimeTrackingError,
};

/// Outbound port for time tracking operations.
///
/// This trait defines the contract that any time tracking provider
/// (Milltime, or future providers) must implement.
///
/// Note: The client is created per-request with the user's credentials,
/// so user_id is not passed to individual methods.
#[async_trait]
pub trait TimeTrackingClient: Send + Sync + 'static {
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
    async fn get_time_info(&self, date_range: (Date, Date)) -> Result<TimeInfo, TimeTrackingError>;

    /// Get time entries from the provider for a date range.
    ///
    /// Returns raw entries without local database augmentation.
    async fn get_time_entries(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntry>, TimeTrackingError>;

    /// Create a new time entry.
    ///
    /// Returns the registration ID from the provider.
    async fn create_time_entry(
        &self,
        request: &CreateTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError>;

    /// Edit an existing time entry.
    ///
    /// If the registration day changes, the provider may need to delete
    /// and recreate the entry. Returns the (possibly new) registration ID.
    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError>;

    /// Delete a time entry.
    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError>;
}
