use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::domain::{
    models::{
        BeginTimeEntryWrite, CreateTimeEntryRequest, PendingTimeEntryWrite, PreparedTimeEntry,
        TimeEntry, TimeEntryWriteIntent, TimeEntryWriteOperation, TimeEntryWriteOrigin,
        TimeEntryWriteResolution, UserId,
    },
    ports::outbound::{TimeEntryWriteStore, TimeTrackingClient, TimerHistoryRepository},
    TimeTrackingError,
};

use super::TimeTrackingServiceImpl;

impl<C, R> TimeTrackingServiceImpl<C, R>
where
    C: TimeTrackingClient,
    R: TimerHistoryRepository + TimeEntryWriteStore,
{
    pub(super) async fn save_active_time_entry(
        &self,
        user_id: &UserId,
        note_override: Option<String>,
        key: &str,
    ) -> Result<TimeEntry, TimeTrackingError> {
        let key = validate_key(key)?;
        let operation = TimeEntryWriteOperation::SaveActiveTimer;
        let begin = self
            .timer_repo
            .begin(TimeEntryWriteIntent::SaveActiveTimer {
                user_id: *user_id,
                key: key.to_string(),
                fingerprint: save_fingerprint(note_override.as_deref()),
                operation_id: operation_id(user_id, operation, key),
                stopped_at: OffsetDateTime::now_utc(),
                note_override,
            })
            .await?;

        if let BeginTimeEntryWrite::Fresh(write) = &begin {
            if let Err(error) = self.validate_prepared_write(&write.entry).await {
                self.timer_repo
                    .resolve(write, TimeEntryWriteResolution::Cancel)
                    .await?;
                return Err(error);
            }
        }

        self.execute_prepared_write(begin).await
    }

    pub(super) async fn create_prepared_time_entry(
        &self,
        user_id: &UserId,
        request: &CreateTimeEntryRequest,
        key: &str,
    ) -> Result<TimeEntry, TimeTrackingError> {
        let key = validate_key(key)?;
        Self::validate_interval(request.start_time, request.end_time)?;
        self.ensure_day_is_open(request.start_time.date()).await?;
        let (project, activity) = self
            .resolve_required_selection(
                &request.project_id,
                &request.activity_id,
                request.start_time.date(),
            )
            .await?;
        let entry = PreparedTimeEntry {
            origin: TimeEntryWriteOrigin::Direct,
            project_id: project.id,
            project_name: project.name,
            activity_id: activity.id,
            activity_name: activity.name,
            start_time: request.start_time,
            end_time: request.end_time,
            note: request.note.clone(),
        };
        let operation = TimeEntryWriteOperation::CreateTimeEntry;
        let begin = self
            .timer_repo
            .begin(TimeEntryWriteIntent::CreateTimeEntry {
                user_id: *user_id,
                key: key.to_string(),
                fingerprint: create_fingerprint(request),
                operation_id: operation_id(user_id, operation, key),
                entry,
            })
            .await?;

        self.execute_prepared_write(begin).await
    }

    async fn validate_prepared_write(
        &self,
        entry: &PreparedTimeEntry,
    ) -> Result<(), TimeTrackingError> {
        Self::validate_interval(entry.start_time, entry.end_time)?;
        self.ensure_day_is_open(entry.start_time.date()).await?;
        self.resolve_required_selection(
            &entry.project_id,
            &entry.activity_id,
            entry.start_time.date(),
        )
        .await?;
        Ok(())
    }

    async fn execute_prepared_write(
        &self,
        begin: BeginTimeEntryWrite,
    ) -> Result<TimeEntry, TimeTrackingError> {
        match begin {
            BeginTimeEntryWrite::Fresh(write) => {
                let request = write.entry.provider_request();
                match self
                    .client
                    .create_time_entry(&request, &write.operation_id)
                    .await
                {
                    Ok(registration_id) => {
                        self.complete_write(&write, registration_id.as_str()).await
                    }
                    Err(error) if provider_definitively_rejected(&error) => {
                        self.timer_repo
                            .resolve(&write, TimeEntryWriteResolution::Cancel)
                            .await?;
                        Err(error)
                    }
                    // The provider may have committed before a timeout or transport failure.
                    // Keeping the command pending makes every retry reconciliation-only.
                    Err(error) => Err(error),
                }
            }
            BeginTimeEntryWrite::Pending(write) => {
                // Provider operation IDs may be lookup handles rather than uniqueness
                // constraints. Never resubmit after an ambiguous first outcome.
                let registration_id = self
                    .client
                    .find_time_entry_by_operation_id(
                        &write.operation_id,
                        write.entry.start_time.date(),
                    )
                    .await?
                    .ok_or(TimeTrackingError::IdempotencyInProgress)?;
                self.complete_write(&write, registration_id.as_str()).await
            }
            BeginTimeEntryWrite::Replay {
                entry,
                registration_id,
            } => Ok(entry.completed(registration_id)),
            BeginTimeEntryWrite::NoActiveTimer => Err(TimeTrackingError::NoTimerRunning),
            BeginTimeEntryWrite::PayloadMismatch => Err(TimeTrackingError::IdempotencyConflict),
        }
    }

    async fn complete_write(
        &self,
        write: &PendingTimeEntryWrite,
        registration_id: &str,
    ) -> Result<TimeEntry, TimeTrackingError> {
        self.timer_repo
            .resolve(write, TimeEntryWriteResolution::Complete(registration_id))
            .await?;
        Ok(write.entry.completed(registration_id))
    }
}

fn validate_key(key: &str) -> Result<&str, TimeTrackingError> {
    let key = key.trim();
    if key.is_empty() || key.len() > 200 {
        return Err(TimeTrackingError::InvalidInput(
            "Idempotency-Key must contain between 1 and 200 characters".to_string(),
        ));
    }
    Ok(key)
}

fn operation_id(user_id: &UserId, operation: TimeEntryWriteOperation, key: &str) -> String {
    let source = format!("{}:{}:{key}", user_id.as_i32(), operation.as_str());
    format!("toki-op-{}", sha256_hex(source.as_bytes()))
}

fn save_fingerprint(note_override: Option<&str>) -> String {
    fingerprint(b"save-active-timer:v1", [note_override.map(str::as_bytes)])
}

fn create_fingerprint(request: &CreateTimeEntryRequest) -> String {
    let start = request.start_time.unix_timestamp_nanos().to_string();
    let end = request.end_time.unix_timestamp_nanos().to_string();
    fingerprint(
        b"create-time-entry:v1",
        [
            Some(request.project_id.as_str().as_bytes()),
            Some(request.activity_id.as_str().as_bytes()),
            Some(start.as_bytes()),
            Some(end.as_bytes()),
            Some(request.note.as_bytes()),
        ],
    )
}

fn fingerprint<'a>(schema: &[u8], fields: impl IntoIterator<Item = Option<&'a [u8]>>) -> String {
    let mut digest = Sha256::new();
    digest.update((schema.len() as u64).to_be_bytes());
    digest.update(schema);
    for field in fields {
        match field {
            Some(value) => {
                digest.update([1]);
                digest.update((value.len() as u64).to_be_bytes());
                digest.update(value);
            }
            None => digest.update([0]),
        }
    }
    hex_digest(&digest.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(&Sha256::digest(bytes))
}

fn provider_definitively_rejected(error: &TimeTrackingError) -> bool {
    matches!(
        error,
        TimeTrackingError::InvalidInput(_)
            | TimeTrackingError::InvalidProjectActivity(_)
            | TimeTrackingError::LockedPeriod
            | TimeTrackingError::Forbidden(_)
            | TimeTrackingError::ProjectNotFound(_)
            | TimeTrackingError::ActivityNotFound(_)
    )
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;
    use crate::domain::models::{ActivityId, ProjectId};
    use time::{Date, Duration};

    #[test]
    fn fingerprints_distinguish_missing_empty_and_field_boundaries() {
        assert_ne!(save_fingerprint(None), save_fingerprint(Some("")));
        assert_ne!(
            fingerprint(b"v1", [Some(b"ab".as_slice()), Some(b"c".as_slice())]),
            fingerprint(b"v1", [Some(b"a".as_slice()), Some(b"bc".as_slice())])
        );
    }

    #[test]
    fn create_fingerprint_is_stable_and_changes_with_wire_fields() {
        let start = Date::from_calendar_date(2026, time::Month::August, 25)
            .unwrap()
            .with_hms(9, 0, 0)
            .unwrap()
            .assume_utc();
        let request = CreateTimeEntryRequest {
            project_id: ProjectId::new("project-1"),
            activity_id: ActivityId::new("activity-1"),
            start_time: start,
            end_time: start + Duration::hours(1),
            note: "work".to_string(),
        };
        assert_eq!(create_fingerprint(&request), create_fingerprint(&request));
        let mut changed = request.clone();
        changed.note.push('!');
        assert_ne!(create_fingerprint(&request), create_fingerprint(&changed));
    }
}
