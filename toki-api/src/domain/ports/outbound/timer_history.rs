//! Timer history repository port (outbound).
//!
//! Defines the interface for persisting local timer history records.

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{NewTimerHistoryEntry, TimerHistoryEntry, TimerHistoryId, UserId},
    TimeTrackingError,
};

/// No-op implementation of TimerHistoryRepository.
///
/// Used when the service is created without a timer history repository.
#[async_trait]
impl TimerHistoryRepository for () {
    async fn get_history(&self, _user_id: &UserId) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        Ok(Vec::new())
    }

    async fn get_by_registration_id(
        &self,
        _registration_id: &str,
    ) -> Result<Option<TimerHistoryEntry>, TimeTrackingError> {
        Ok(None)
    }

    async fn create_finished(
        &self,
        _entry: &NewTimerHistoryEntry,
    ) -> Result<TimerHistoryId, TimeTrackingError> {
        Ok(TimerHistoryId::new(0))
    }

    async fn update_times(
        &self,
        _registration_id: &str,
        _start_time: &OffsetDateTime,
        _end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        Ok(())
    }

    async fn update_registration_and_times(
        &self,
        _old_registration_id: &str,
        _new_registration_id: &str,
        _start_time: &OffsetDateTime,
        _end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        Ok(())
    }
}

/// Outbound port for timer history persistence.
///
/// This trait abstracts the local database storage of timer history,
/// allowing the service layer to track start/end times independently
/// of the external time tracking provider.
#[async_trait]
pub trait TimerHistoryRepository: Send + Sync + 'static {
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
