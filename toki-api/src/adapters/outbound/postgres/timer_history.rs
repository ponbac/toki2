//! PostgreSQL implementation of the TimerHistoryRepository port.

use std::sync::Arc;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        ActivityId, NewTimerHistoryEntry, ProjectId, TimerHistoryEntry, TimerHistoryId,
        UserId,
    },
    ports::outbound::TimerHistoryRepository,
    TimeTrackingError,
};
use crate::repositories::{DatabaseTimer, FinishedDatabaseTimer, TimerRepository, TimerRepositoryImpl};

/// Adapter that implements TimerHistoryRepository using PostgreSQL.
pub struct PostgresTimerHistoryAdapter<R = TimerRepositoryImpl> {
    repo: Arc<R>,
}

impl<R> PostgresTimerHistoryAdapter<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl<R: TimerRepository + Send + Sync + 'static> TimerHistoryRepository
    for PostgresTimerHistoryAdapter<R>
{
    async fn get_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        let timers = self
            .repo
            .get_timer_history(&user_id.as_i32())
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))?;

        Ok(timers.into_iter().map(db_timer_to_domain).collect())
    }

    async fn get_by_registration_id(
        &self,
        registration_id: &str,
    ) -> Result<Option<TimerHistoryEntry>, TimeTrackingError> {
        let timer = self
            .repo
            .get_by_registration_id(registration_id)
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))?;

        Ok(timer.map(db_timer_to_domain))
    }

    async fn create_finished(
        &self,
        entry: &NewTimerHistoryEntry,
    ) -> Result<TimerHistoryId, TimeTrackingError> {
        let timer = FinishedDatabaseTimer {
            user_id: entry.user_id.as_i32(),
            start_time: entry.start_time,
            end_time: entry.end_time,
            project_id: entry.project_id.as_ref().map(|p| p.to_string()),
            project_name: entry.project_name.clone(),
            activity_id: entry.activity_id.as_ref().map(|a| a.to_string()),
            activity_name: entry.activity_name.clone(),
            note: entry.note.clone(),
            registration_id: entry.registration_id.clone(),
        };

        let id = self
            .repo
            .create_finished_timer(&timer)
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))?;

        Ok(TimerHistoryId::new(id))
    }

    async fn update_times(
        &self,
        registration_id: &str,
        start_time: &OffsetDateTime,
        end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        self.repo
            .update_start_and_end_time(registration_id, start_time, end_time)
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))
    }

    async fn update_registration_and_times(
        &self,
        old_registration_id: &str,
        new_registration_id: &str,
        start_time: &OffsetDateTime,
        end_time: &OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        self.repo
            .update_times_and_registration_id(
                old_registration_id,
                new_registration_id,
                start_time,
                end_time,
            )
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))
    }
}

/// Convert a database timer to a domain TimerHistoryEntry.
fn db_timer_to_domain(timer: DatabaseTimer) -> TimerHistoryEntry {
    let mut entry = TimerHistoryEntry::new(
        timer.id,
        timer.user_id,
        timer.start_time,
        timer.created_at,
    );

    if let Some(reg_id) = timer.registration_id {
        entry = entry.with_registration_id(reg_id);
    }

    if let Some(end_time) = timer.end_time {
        entry = entry.with_end_time(end_time);
    }

    if let (Some(project_id), Some(project_name)) = (timer.project_id, timer.project_name) {
        entry = entry.with_project(ProjectId::new(project_id), project_name);
    }

    if let (Some(activity_id), Some(activity_name)) = (timer.activity_id, timer.activity_name) {
        entry = entry.with_activity(ActivityId::new(activity_id), activity_name);
    }

    if let Some(note) = timer.note {
        entry = entry.with_note(note);
    }

    entry
}
