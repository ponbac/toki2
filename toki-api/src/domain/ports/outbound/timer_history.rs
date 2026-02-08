//! Timer history repository port (outbound).
//!
//! Defines the interface for persisting local timer history records
//! and managing active timers.

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{ActiveTimer, NewTimerHistoryEntry, TimerHistoryEntry, TimerHistoryId, UserId},
    TimeTrackingError,
};

/// Outbound port for timer history persistence.
///
/// This trait abstracts the local database storage of timer history
/// and active timers, allowing the service layer to track start/end
/// times independently of the external time tracking provider.
#[async_trait]
pub trait TimerHistoryRepository: Send + Sync + 'static {
    // ========================================================================
    // Active Timer Operations
    // ========================================================================

    /// Get the currently active (unsaved) timer for a user, if any.
    async fn get_active_timer(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ActiveTimer>, TimeTrackingError>;

    /// Create a new active timer for a user.
    async fn create_timer(
        &self,
        user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError>;

    /// Update the active timer for a user.
    async fn update_timer(
        &self,
        user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError>;

    /// Delete the active timer for a user (stop without saving).
    async fn delete_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError>;

    /// Mark the active timer as finished by setting end_time and registration_id.
    async fn save_timer_finished(
        &self,
        user_id: &UserId,
        end_time: &OffsetDateTime,
        registration_id: &str,
    ) -> Result<(), TimeTrackingError>;

    // ========================================================================
    // Timer History Operations
    // ========================================================================

    /// Get all timer history entries for a user.
    async fn get_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError>;

    /// Get a timer history entry by its registration ID (from the provider).
    async fn get_by_registration_id(
        &self,
        registration_id: &str,
    ) -> Result<Option<TimerHistoryEntry>, TimeTrackingError>;

    /// Create a finished timer history entry.
    ///
    /// Returns the ID of the created entry.
    async fn create_finished(
        &self,
        entry: &NewTimerHistoryEntry,
    ) -> Result<TimerHistoryId, TimeTrackingError>;

    /// Update the start and end times for a timer entry.
    async fn update_times(
        &self,
        registration_id: &str,
        start_time: &OffsetDateTime,
        end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError>;

    /// Update the registration ID and times for a timer entry.
    ///
    /// Used when the registration date changes (delete + recreate in provider).
    async fn update_registration_and_times(
        &self,
        old_registration_id: &str,
        new_registration_id: &str,
        start_time: &OffsetDateTime,
        end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError>;
}
