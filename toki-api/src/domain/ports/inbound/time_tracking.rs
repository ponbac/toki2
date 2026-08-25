use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, ActiveTimer, Activity,
        CreateAbsencesRequest, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId,
        StartTimerRequest, TimeEntry, TimeEntryDayStatus, TimerHistoryEntry, UpdateTimerRequest,
        UserId, WeeklyStats,
    },
    TimeTrackingError,
};

/// Inbound port for time tracking operations.
///
/// This trait defines the use cases that HTTP handlers can invoke.
/// It orchestrates the outbound ports (clients, repositories) to fulfill requests.
///
/// Note: The service is created per-request with the user's credentials,
/// so user_id is not passed to methods that only interact with the provider.
#[async_trait]
pub trait TimeTrackingService: Send + Sync + 'static {
    // ========================================================================
    // Active Timer Operations (local DB)
    // ========================================================================

    /// Get the currently running timer for a user, if any.
    async fn get_active_timer(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ActiveTimer>, TimeTrackingError>;

    /// Start a new timer for a user.
    async fn start_timer(
        &self,
        user_id: &UserId,
        request: &StartTimerRequest,
    ) -> Result<ActiveTimer, TimeTrackingError>;

    /// Stop the currently running timer without saving to the provider.
    async fn stop_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError>;

    /// Save/register the current timer as a time entry in the provider.
    ///
    /// Orchestrates: get active timer → compute times → create entry in provider → mark finished locally.
    async fn save_timer(
        &self,
        user_id: &UserId,
        note: Option<String>,
        idempotency_key: &str,
    ) -> Result<TimeEntry, TimeTrackingError>;

    /// Edit the active timer for a user.
    async fn edit_timer(
        &self,
        user_id: &UserId,
        request: &UpdateTimerRequest,
    ) -> Result<ActiveTimer, TimeTrackingError>;

    // ========================================================================
    // Project/Activity Lookups
    // ========================================================================

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
    ) -> Result<WeeklyStats, TimeTrackingError>;

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

    /// Get date-level statuses for a date range.
    async fn get_time_entry_day_statuses(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntryDayStatus>, TimeTrackingError>;

    /// Create a new time entry.
    ///
    /// Creates the entry in the provider and persists to local timer history.
    async fn create_time_entry(
        &self,
        user_id: &UserId,
        request: &CreateTimeEntryRequest,
        idempotency_key: &str,
    ) -> Result<TimeEntry, TimeTrackingError>;

    /// Edit an existing time entry.
    ///
    /// Updates both the provider and local timer history.
    async fn edit_time_entry(
        &self,
        registration_id: &str,
        request: &EditTimeEntryRequest,
    ) -> Result<TimeEntry, TimeTrackingError>;

    /// Delete a time entry.
    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError>;

    /// Get available provider-backed absence types.
    async fn get_absence_types(&self) -> Result<Vec<AbsenceType>, TimeTrackingError>;

    /// Get registered children available for child-related absence reporting.
    async fn get_absence_children(&self) -> Result<Vec<AbsenceChild>, TimeTrackingError>;

    /// Get absence entries for a date range.
    async fn get_absences(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError>;

    /// Get scheduled-hour defaults for absence reporting.
    async fn get_absence_day_defaults(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceDayDefault>, TimeTrackingError>;

    /// Create absence entries.
    async fn create_absences(
        &self,
        request: &CreateAbsencesRequest,
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError>;

    /// Delete a managed absence entry.
    async fn delete_absence(&self, absence_id: &str, date: Date) -> Result<(), TimeTrackingError>;

    /// Get timer history entries for a user.
    async fn get_timer_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError>;
}
