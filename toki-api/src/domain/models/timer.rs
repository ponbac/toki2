use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};

use super::{ActivityId, ProjectId, TimerHistoryId, UserId};

/// A currently running timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTimer {
    pub started_at: OffsetDateTime,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<String>,
    pub activity_id: Option<ActivityId>,
    pub activity_name: Option<String>,
    pub note: String,
}

impl ActiveTimer {
    pub fn new(started_at: OffsetDateTime) -> Self {
        Self {
            started_at,
            project_id: None,
            project_name: None,
            activity_id: None,
            activity_name: None,
            note: String::new(),
        }
    }

    pub fn with_project(mut self, id: impl Into<ProjectId>, name: impl Into<String>) -> Self {
        self.project_id = Some(id.into());
        self.project_name = Some(name.into());
        self
    }

    pub fn with_activity(mut self, id: impl Into<ActivityId>, name: impl Into<String>) -> Self {
        self.activity_id = Some(id.into());
        self.activity_name = Some(name.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }

    /// Calculate elapsed time since the timer started.
    pub fn elapsed(&self) -> Duration {
        OffsetDateTime::now_utc() - self.started_at
    }

    /// Get elapsed time as (hours, minutes, seconds).
    pub fn elapsed_hms(&self) -> (i64, i64, i64) {
        let elapsed = self.elapsed();
        let total_seconds = elapsed.whole_seconds();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        (hours, minutes, seconds)
    }
}

/// Attestation level for time entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeEntryStatus {
    #[default]
    Open,
    Approved,
    Certified,
}

/// Date-level attestation status for time entry creation/editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeEntryDayStatus {
    pub date: Date,
    pub status: TimeEntryStatus,
}

/// A completed time entry.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeEntry {
    pub registration_id: String,
    pub project_id: ProjectId,
    pub project_name: String,
    pub activity_id: ActivityId,
    pub activity_name: String,
    pub date: Date,
    pub hours: f64,
    pub note: Option<String>,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub week_number: u8,
    pub status: TimeEntryStatus,
}

impl TimeEntry {
    pub fn new(
        registration_id: impl Into<String>,
        project_id: impl Into<ProjectId>,
        project_name: impl Into<String>,
        activity_id: impl Into<ActivityId>,
        activity_name: impl Into<String>,
        date: Date,
        hours: f64,
    ) -> Self {
        Self {
            registration_id: registration_id.into(),
            project_id: project_id.into(),
            project_name: project_name.into(),
            activity_id: activity_id.into(),
            activity_name: activity_name.into(),
            date,
            hours,
            note: None,
            start_time: None,
            end_time: None,
            week_number: 0,
            status: TimeEntryStatus::Open,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn with_times(
        mut self,
        start_time: Option<OffsetDateTime>,
        end_time: Option<OffsetDateTime>,
    ) -> Self {
        self.start_time = start_time;
        self.end_time = end_time;
        self
    }

    pub fn with_week_number(mut self, week_number: u8) -> Self {
        self.week_number = week_number;
        self
    }

    pub fn with_status(mut self, status: TimeEntryStatus) -> Self {
        self.status = status;
        self
    }
}

/// Time tracking statistics for a period.
#[derive(Debug, Clone, PartialEq)]
pub struct WeeklyStats {
    pub worked_hours: f64,
    pub scheduled_hours: f64,
    pub remaining_hours: f64,
    pub absence_hours: f64,
    pub covered_hours: f64,
    pub period_flex_hours: f64,
}

impl WeeklyStats {
    pub fn new(worked_hours: f64, scheduled_hours: f64, absence_hours: f64) -> Self {
        let covered_hours = worked_hours + absence_hours;
        Self {
            worked_hours,
            scheduled_hours,
            absence_hours,
            covered_hours,
            remaining_hours: (scheduled_hours - covered_hours).max(0.0),
            period_flex_hours: covered_hours - scheduled_hours,
        }
    }
}

/// Request to create a new time entry directly (without timer).
#[derive(Debug, Clone)]
pub struct CreateTimeEntryRequest {
    pub project_id: ProjectId,
    pub project_name: String,
    pub activity_id: ActivityId,
    pub activity_name: String,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub note: String,
}

/// Request to edit an existing time entry.
#[derive(Debug, Clone)]
pub struct EditTimeEntryRequest {
    pub registration_id: String,
    pub project_id: ProjectId,
    pub activity_id: ActivityId,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub note: String,
}

/// A local timer history entry (stored in our database).
///
/// This tracks start/end times for time entries that may also exist
/// in the external time tracking provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerHistoryEntry {
    pub id: TimerHistoryId,
    pub user_id: UserId,
    /// Registration ID from the time tracking provider (if saved).
    pub registration_id: Option<String>,
    pub start_time: OffsetDateTime,
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<String>,
    pub activity_id: Option<ActivityId>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    pub created_at: OffsetDateTime,
}

impl TimerHistoryEntry {
    pub fn new(
        id: impl Into<TimerHistoryId>,
        user_id: impl Into<UserId>,
        start_time: OffsetDateTime,
        created_at: OffsetDateTime,
    ) -> Self {
        Self {
            id: id.into(),
            user_id: user_id.into(),
            registration_id: None,
            start_time,
            end_time: None,
            project_id: None,
            project_name: None,
            activity_id: None,
            activity_name: None,
            note: None,
            created_at,
        }
    }

    pub fn with_registration_id(mut self, id: impl Into<String>) -> Self {
        self.registration_id = Some(id.into());
        self
    }

    pub fn with_end_time(mut self, end_time: OffsetDateTime) -> Self {
        self.end_time = Some(end_time);
        self
    }

    pub fn with_project(mut self, id: impl Into<ProjectId>, name: impl Into<String>) -> Self {
        self.project_id = Some(id.into());
        self.project_name = Some(name.into());
        self
    }

    pub fn with_activity(mut self, id: impl Into<ActivityId>, name: impl Into<String>) -> Self {
        self.activity_id = Some(id.into());
        self.activity_name = Some(name.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Data for creating a new finished timer history entry.
#[derive(Debug, Clone)]
pub struct NewTimerHistoryEntry {
    pub user_id: UserId,
    pub registration_id: String,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<String>,
    pub activity_id: Option<ActivityId>,
    pub activity_name: Option<String>,
    pub note: String,
}
