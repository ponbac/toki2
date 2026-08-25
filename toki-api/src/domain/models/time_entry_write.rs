use time::OffsetDateTime;

use super::{
    ActivityId, CreateTimeEntryRequest, ProjectId, TimeEntry, TimeEntryStatus, TimerHistoryId,
    UserId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeEntryWriteOperation {
    SaveActiveTimer,
    CreateTimeEntry,
}

impl TimeEntryWriteOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SaveActiveTimer => "save_active_timer",
            Self::CreateTimeEntry => "create_time_entry",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeEntryWriteOrigin {
    ActiveTimer(TimerHistoryId),
    Direct,
}

/// The exact provider command persisted before its first submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTimeEntry {
    pub origin: TimeEntryWriteOrigin,
    pub project_id: ProjectId,
    pub project_name: String,
    pub activity_id: ActivityId,
    pub activity_name: String,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub note: String,
}

impl PreparedTimeEntry {
    pub fn provider_request(&self) -> CreateTimeEntryRequest {
        CreateTimeEntryRequest {
            project_id: self.project_id.clone(),
            activity_id: self.activity_id.clone(),
            start_time: self.start_time,
            end_time: self.end_time,
            note: self.note.clone(),
        }
    }

    pub fn completed(&self, registration_id: impl Into<String>) -> TimeEntry {
        let date = self.start_time.date();
        TimeEntry::new(
            registration_id,
            self.project_id.clone(),
            self.project_name.clone(),
            self.activity_id.clone(),
            self.activity_name.clone(),
            date,
            (self.end_time - self.start_time).whole_seconds() as f64 / 3600.0,
        )
        .with_note(self.note.clone())
        .with_times(Some(self.start_time), Some(self.end_time))
        .with_week_number(date.iso_week())
        .with_status(TimeEntryStatus::Open)
    }
}

#[derive(Debug, Clone)]
pub enum TimeEntryWriteIntent {
    SaveActiveTimer {
        user_id: UserId,
        key: String,
        fingerprint: String,
        operation_id: String,
        stopped_at: OffsetDateTime,
        note_override: Option<String>,
    },
    CreateTimeEntry {
        user_id: UserId,
        key: String,
        fingerprint: String,
        operation_id: String,
        entry: PreparedTimeEntry,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTimeEntryWrite {
    pub user_id: UserId,
    pub operation: TimeEntryWriteOperation,
    pub key: String,
    pub operation_id: String,
    pub entry: PreparedTimeEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginTimeEntryWrite {
    Fresh(PendingTimeEntryWrite),
    Pending(PendingTimeEntryWrite),
    Replay {
        entry: PreparedTimeEntry,
        registration_id: String,
    },
    NoActiveTimer,
    PayloadMismatch,
}

pub enum TimeEntryWriteResolution<'a> {
    Complete(&'a str),
    Cancel,
}
