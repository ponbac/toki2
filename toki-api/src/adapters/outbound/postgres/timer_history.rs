//! PostgreSQL implementation of the TimerHistoryRepository port.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::domain::{
    models::{
        ActiveTimer, ActivityId, BeginTimeEntryWrite, PendingTimeEntryWrite, PreparedTimeEntry,
        ProjectId, TimeEntryWriteIntent, TimeEntryWriteOperation, TimeEntryWriteOrigin,
        TimeEntryWriteResolution, TimerHistoryEntry, TimerHistoryId, UserId,
    },
    ports::outbound::{TimeEntryWriteStore, TimerHistoryRepository},
    TimeTrackingError,
};
use crate::repositories::{
    DatabaseBeginTimeEntryWrite, DatabaseTimer, FinishedDatabaseTimer, NewDatabaseTimer,
    TimeEntryWriteRepository, TimerRepository, TimerRepositoryImpl, UpdateDatabaseTimer,
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
            .map_err(storage_error)?;

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
                storage_error(error)
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

        self.repo.update_timer(&update).await.map_err(storage_error)
    }

    async fn delete_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError> {
        self.repo
            .delete_active_timer(&user_id.as_i32())
            .await
            .map_err(storage_error)
    }

    async fn get_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        let timers = self
            .repo
            .get_timer_history(&user_id.as_i32())
            .await
            .map_err(storage_error)?;

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
            .map_err(storage_error)?;

        Ok(timer.map(db_timer_to_domain))
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
            .map_err(storage_error)
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
            .map_err(storage_error)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPreparedTimeEntryV1 {
    version: u8,
    origin: StoredWriteOriginV1,
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    #[serde(with = "time::serde::rfc3339")]
    start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    end_time: OffsetDateTime,
    note: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum StoredWriteOriginV1 {
    ActiveTimer {
        #[serde(rename = "timerHistoryId")]
        timer_history_id: i32,
    },
    Direct,
}

impl StoredPreparedTimeEntryV1 {
    fn from_domain(entry: &PreparedTimeEntry) -> Self {
        Self {
            version: 1,
            origin: match entry.origin {
                TimeEntryWriteOrigin::ActiveTimer(id) => StoredWriteOriginV1::ActiveTimer {
                    timer_history_id: id.as_i32(),
                },
                TimeEntryWriteOrigin::Direct => StoredWriteOriginV1::Direct,
            },
            project_id: entry.project_id.to_string(),
            project_name: entry.project_name.clone(),
            activity_id: entry.activity_id.to_string(),
            activity_name: entry.activity_name.clone(),
            start_time: entry.start_time,
            end_time: entry.end_time,
            note: entry.note.clone(),
        }
    }

    fn into_domain(self) -> Result<PreparedTimeEntry, TimeTrackingError> {
        if self.version != 1 {
            return Err(TimeTrackingError::storage_unavailable(format!(
                "unsupported prepared time-entry version {}",
                self.version
            )));
        }
        Ok(PreparedTimeEntry {
            origin: match self.origin {
                StoredWriteOriginV1::ActiveTimer { timer_history_id } => {
                    TimeEntryWriteOrigin::ActiveTimer(TimerHistoryId::new(timer_history_id))
                }
                StoredWriteOriginV1::Direct => TimeEntryWriteOrigin::Direct,
            },
            project_id: ProjectId::new(self.project_id),
            project_name: self.project_name,
            activity_id: ActivityId::new(self.activity_id),
            activity_name: self.activity_name,
            start_time: self.start_time,
            end_time: self.end_time,
            note: self.note,
        })
    }
}

fn decode_prepared(value: serde_json::Value) -> Result<PreparedTimeEntry, TimeTrackingError> {
    serde_json::from_value::<StoredPreparedTimeEntryV1>(value)
        .map_err(|error| {
            TimeTrackingError::storage_unavailable(format!(
                "invalid persisted prepared time entry: {error}"
            ))
        })?
        .into_domain()
}

fn pending_write(
    user_id: UserId,
    operation: TimeEntryWriteOperation,
    key: String,
    operation_id: String,
    prepared: serde_json::Value,
) -> Result<PendingTimeEntryWrite, TimeTrackingError> {
    let entry = decode_prepared(prepared)?;
    validate_origin(operation, &entry.origin)?;
    Ok(PendingTimeEntryWrite {
        user_id,
        operation,
        key,
        operation_id,
        entry,
    })
}

fn validate_origin(
    operation: TimeEntryWriteOperation,
    origin: &TimeEntryWriteOrigin,
) -> Result<(), TimeTrackingError> {
    if matches!(
        (operation, origin),
        (
            TimeEntryWriteOperation::SaveActiveTimer,
            TimeEntryWriteOrigin::ActiveTimer(_)
        ) | (
            TimeEntryWriteOperation::CreateTimeEntry,
            TimeEntryWriteOrigin::Direct
        )
    ) {
        Ok(())
    } else {
        Err(TimeTrackingError::storage_unavailable(
            "persisted time-entry origin does not match its operation",
        ))
    }
}

fn map_begin(
    value: DatabaseBeginTimeEntryWrite,
    user_id: UserId,
    operation: TimeEntryWriteOperation,
    key: String,
) -> Result<BeginTimeEntryWrite, TimeTrackingError> {
    Ok(match value {
        DatabaseBeginTimeEntryWrite::Fresh {
            operation_id,
            prepared,
        } => BeginTimeEntryWrite::Fresh(pending_write(
            user_id,
            operation,
            key,
            operation_id,
            prepared,
        )?),
        DatabaseBeginTimeEntryWrite::Pending {
            operation_id,
            prepared,
        } => BeginTimeEntryWrite::Pending(pending_write(
            user_id,
            operation,
            key,
            operation_id,
            prepared,
        )?),
        DatabaseBeginTimeEntryWrite::Replay {
            prepared,
            registration_id,
        } => {
            let entry = decode_prepared(prepared)?;
            validate_origin(operation, &entry.origin)?;
            BeginTimeEntryWrite::Replay {
                entry,
                registration_id,
            }
        }
        DatabaseBeginTimeEntryWrite::NoActiveTimer => BeginTimeEntryWrite::NoActiveTimer,
        DatabaseBeginTimeEntryWrite::PayloadMismatch => BeginTimeEntryWrite::PayloadMismatch,
    })
}

#[async_trait]
impl<R: TimeEntryWriteRepository + Send + Sync + 'static> TimeEntryWriteStore
    for PostgresTimerHistoryAdapter<R>
{
    async fn begin(
        &self,
        intent: TimeEntryWriteIntent,
    ) -> Result<BeginTimeEntryWrite, TimeTrackingError> {
        let (result, user_id, operation, key) = match intent {
            TimeEntryWriteIntent::SaveActiveTimer {
                user_id,
                key,
                fingerprint,
                operation_id,
                stopped_at,
                note_override,
            } => (
                self.repo
                    .begin_save_active_timer(
                        user_id.as_i32(),
                        &key,
                        &fingerprint,
                        &operation_id,
                        stopped_at,
                        note_override.as_deref(),
                    )
                    .await
                    .map_err(storage_error)?,
                user_id,
                TimeEntryWriteOperation::SaveActiveTimer,
                key,
            ),
            TimeEntryWriteIntent::CreateTimeEntry {
                user_id,
                key,
                fingerprint,
                operation_id,
                entry,
            } => {
                let prepared = serde_json::to_value(StoredPreparedTimeEntryV1::from_domain(&entry))
                    .map_err(|error| TimeTrackingError::storage_unavailable(error.to_string()))?;
                (
                    self.repo
                        .begin_create_time_entry(
                            user_id.as_i32(),
                            &key,
                            &fingerprint,
                            &operation_id,
                            prepared,
                        )
                        .await
                        .map_err(storage_error)?,
                    user_id,
                    TimeEntryWriteOperation::CreateTimeEntry,
                    key,
                )
            }
        };
        map_begin(result, user_id, operation, key)
    }

    async fn resolve(
        &self,
        write: &PendingTimeEntryWrite,
        resolution: TimeEntryWriteResolution<'_>,
    ) -> Result<(), TimeTrackingError> {
        match resolution {
            TimeEntryWriteResolution::Cancel => self
                .repo
                .cancel_time_entry_write(
                    write.user_id.as_i32(),
                    write.operation.as_str(),
                    &write.key,
                )
                .await
                .map_err(storage_error),
            TimeEntryWriteResolution::Complete(registration_id) => {
                let timer = FinishedDatabaseTimer {
                    user_id: write.user_id.as_i32(),
                    start_time: write.entry.start_time,
                    end_time: write.entry.end_time,
                    project_id: Some(write.entry.project_id.to_string()),
                    project_name: Some(write.entry.project_name.clone()),
                    activity_id: Some(write.entry.activity_id.to_string()),
                    activity_name: Some(write.entry.activity_name.clone()),
                    note: write.entry.note.clone(),
                    registration_id: registration_id.to_string(),
                };
                let active_timer_id = match write.entry.origin {
                    TimeEntryWriteOrigin::ActiveTimer(id) => Some(id.as_i32()),
                    TimeEntryWriteOrigin::Direct => None,
                };
                self.repo
                    .complete_time_entry_write(
                        write.operation.as_str(),
                        &write.key,
                        active_timer_id,
                        &timer,
                    )
                    .await
                    .map_err(storage_error)
            }
        }
    }
}

fn storage_error(error: impl std::fmt::Display) -> TimeTrackingError {
    TimeTrackingError::storage_unavailable(error.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Duration};

    fn prepared() -> PreparedTimeEntry {
        let start_time = Date::from_calendar_date(2026, time::Month::August, 25)
            .unwrap()
            .with_hms(9, 0, 0)
            .unwrap()
            .assume_utc();
        PreparedTimeEntry {
            origin: TimeEntryWriteOrigin::ActiveTimer(TimerHistoryId::new(42)),
            project_id: ProjectId::new("project-1"),
            project_name: "Project".to_string(),
            activity_id: ActivityId::new("activity-1"),
            activity_name: "Activity".to_string(),
            start_time,
            end_time: start_time + Duration::hours(1),
            note: "work".to_string(),
        }
    }

    #[test]
    fn prepared_write_storage_has_an_explicit_versioned_shape() {
        let value = serde_json::to_value(StoredPreparedTimeEntryV1::from_domain(&prepared()))
            .expect("serialize stored write");

        assert_eq!(value["version"], 1);
        assert_eq!(value["origin"]["kind"], "activeTimer");
        assert_eq!(value["origin"]["timerHistoryId"], 42);
        assert_eq!(decode_prepared(value).unwrap(), prepared());
    }

    #[test]
    fn unknown_prepared_write_versions_fail_closed() {
        let mut value = serde_json::to_value(StoredPreparedTimeEntryV1::from_domain(&prepared()))
            .expect("serialize stored write");
        value["version"] = 2.into();

        assert!(matches!(
            decode_prepared(value),
            Err(TimeTrackingError::StorageUnavailable(_))
        ));
    }
}
