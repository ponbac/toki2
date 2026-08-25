//! PostgreSQL implementation of the TimerHistoryRepository port.

use std::sync::Arc;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        ActiveTimer, ActivityId, IdempotencyClaim, NewTimerHistoryEntry, ProjectId, TimeEntry,
        TimeTrackingWriteOperation, TimerHistoryEntry, TimerHistoryId, UserId,
    },
    ports::outbound::TimerHistoryRepository,
    TimeTrackingError,
};
use crate::repositories::{
    DatabaseIdempotencyClaim, DatabaseTimer, FinishedDatabaseTimer, NewDatabaseTimer,
    TimerRepository, TimerRepositoryImpl, UpdateDatabaseTimer,
};

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
    async fn get_active_timer(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        let timer = self
            .repo
            .active_timer(&user_id.as_i32())
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))?;

        Ok(timer.map(db_timer_to_active_timer))
    }

    async fn create_timer(
        &self,
        user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        let new_timer = NewDatabaseTimer {
            user_id: user_id.as_i32(),
            start_time: timer.started_at,
            project_id: timer.project_id.as_ref().map(|p| p.to_string()),
            project_name: timer.project_name.clone(),
            activity_id: timer.activity_id.as_ref().map(|a| a.to_string()),
            activity_name: timer.activity_name.clone(),
            note: timer.note.clone(),
        };

        self.repo.create_timer(&new_timer).await.map_err(|error| {
            if is_active_timer_unique_violation(&error) {
                TimeTrackingError::TimerAlreadyRunning
            } else {
                TimeTrackingError::unknown(error.to_string())
            }
        })?;

        Ok(())
    }

    async fn update_timer(
        &self,
        user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        let update = UpdateDatabaseTimer {
            user_id: user_id.as_i32(),
            user_note: timer.note.clone(),
            project_id: timer.project_id.as_ref().map(|p| p.to_string()),
            project_name: timer.project_name.clone(),
            activity_id: timer.activity_id.as_ref().map(|a| a.to_string()),
            activity_name: timer.activity_name.clone(),
            start_time: Some(timer.started_at),
        };

        self.repo
            .update_timer(&update)
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))
    }

    async fn delete_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError> {
        self.repo
            .delete_active_timer(&user_id.as_i32())
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))
    }

    async fn finish_timer_idempotently(
        &self,
        user_id: &UserId,
        end_time: &OffsetDateTime,
        registration_id: &str,
        key: &str,
        result: &TimeEntry,
    ) -> Result<(), TimeTrackingError> {
        let result = serde_json::to_value(result)
            .map_err(|error| TimeTrackingError::unknown(error.to_string()))?;
        self.repo
            .finish_active_timer_idempotently(
                &user_id.as_i32(),
                end_time,
                registration_id,
                key,
                result,
            )
            .await
            .map_err(|e| TimeTrackingError::unknown(e.to_string()))
    }

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

    async fn create_finished_idempotently(
        &self,
        entry: &NewTimerHistoryEntry,
        key: &str,
        result: &TimeEntry,
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

        let result = serde_json::to_value(result)
            .map_err(|error| TimeTrackingError::unknown(error.to_string()))?;
        let id = self
            .repo
            .create_finished_timer_idempotently(&timer, key, result)
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

    async fn claim_idempotency(
        &self,
        user_id: &UserId,
        operation: TimeTrackingWriteOperation,
        key: &str,
        request_hash: &str,
        operation_id: &str,
    ) -> Result<IdempotencyClaim, TimeTrackingError> {
        let claim = self
            .repo
            .claim_idempotency(
                user_id.as_i32(),
                operation.as_str(),
                key,
                request_hash,
                operation_id,
            )
            .await
            .map_err(|error| TimeTrackingError::unknown(error.to_string()))?;

        match claim {
            DatabaseIdempotencyClaim::Fresh { operation_id } => {
                Ok(IdempotencyClaim::Fresh { operation_id })
            }
            DatabaseIdempotencyClaim::Resumed { operation_id } => {
                Ok(IdempotencyClaim::Resumed { operation_id })
            }
            DatabaseIdempotencyClaim::Replay(value) => serde_json::from_value::<TimeEntry>(value)
                .map(IdempotencyClaim::Replay)
                .map_err(|error| {
                    TimeTrackingError::unknown(format!(
                        "invalid persisted idempotency result: {error}"
                    ))
                }),
            DatabaseIdempotencyClaim::InProgress => Ok(IdempotencyClaim::InProgress),
            DatabaseIdempotencyClaim::PayloadMismatch => Ok(IdempotencyClaim::PayloadMismatch),
        }
    }

    async fn release_idempotency(
        &self,
        user_id: &UserId,
        operation: TimeTrackingWriteOperation,
        key: &str,
    ) -> Result<(), TimeTrackingError> {
        self.repo
            .release_idempotency(user_id.as_i32(), operation.as_str(), key)
            .await
            .map_err(|error| TimeTrackingError::unknown(error.to_string()))
    }
}

fn is_active_timer_unique_violation(error: &crate::repositories::RepositoryError) -> bool {
    matches!(
        error,
        crate::repositories::RepositoryError::DatabaseError(sqlx::Error::Database(database))
            if database.constraint() == Some("idx_timer_history_one_active_per_user")
    )
}

/// Convert a database timer to a domain ActiveTimer.
fn db_timer_to_active_timer(timer: DatabaseTimer) -> ActiveTimer {
    let mut active = ActiveTimer::new(timer.start_time);

    if let (Some(project_id), Some(project_name)) = (timer.project_id, timer.project_name) {
        active = active.with_project(ProjectId::new(project_id), project_name);
    }

    if let (Some(activity_id), Some(activity_name)) = (timer.activity_id, timer.activity_name) {
        active = active.with_activity(ActivityId::new(activity_id), activity_name);
    }

    if let Some(note) = timer.note {
        active = active.with_note(note);
    }

    active
}

/// Convert a database timer to a domain TimerHistoryEntry.
fn db_timer_to_domain(timer: DatabaseTimer) -> TimerHistoryEntry {
    let mut entry =
        TimerHistoryEntry::new(timer.id, timer.user_id, timer.start_time, timer.created_at);

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
