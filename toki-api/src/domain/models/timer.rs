use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};

use super::{ActivityId, ProjectId, TimerHistoryId, UserId};

/// Source/type of a timer (where it was started).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TimerSource {
    Milltime,
    Standalone,
}

impl std::fmt::Display for TimerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimerSource::Milltime => write!(f, "Milltime"),
            TimerSource::Standalone => write!(f, "Standalone"),
        }
    }
}

impl std::str::FromStr for TimerSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "milltime" => Ok(TimerSource::Milltime),
            "standalone" => Ok(TimerSource::Standalone),
            _ => Err(format!("Unknown timer source: {}", s)),
        }
    }
}

/// A currently running timer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTimer {
    pub source: TimerSource,
    pub started_at: OffsetDateTime,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<String>,
    pub activity_id: Option<ActivityId>,
    pub activity_name: Option<String>,
    pub note: String,
}

impl ActiveTimer {
    pub fn new(source: TimerSource, started_at: OffsetDateTime) -> Self {
        Self {
            source,
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

/// Request to start a new timer.
#[derive(Debug, Clone)]
pub struct StartTimerRequest {
    pub project_id: ProjectId,
    pub project_name: String,
    pub activity_id: ActivityId,
    pub activity_name: String,
    pub note: Option<String>,
    /// Date for the timer registration (YYYY-MM-DD).
    pub reg_day: String,
    /// ISO week number.
    pub week_number: i64,
}

impl StartTimerRequest {
    pub fn new(
        project_id: impl Into<ProjectId>,
        project_name: impl Into<String>,
        activity_id: impl Into<ActivityId>,
        activity_name: impl Into<String>,
        reg_day: impl Into<String>,
        week_number: i64,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            project_name: project_name.into(),
            activity_id: activity_id.into(),
            activity_name: activity_name.into(),
            note: None,
            reg_day: reg_day.into(),
            week_number,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Request to save/stop a running timer.
#[derive(Debug, Clone)]
pub struct SaveTimerRequest {
    pub note: Option<String>,
}

/// Attestation level for time entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum AttestLevel {
    #[default]
    None = 0,
    Week = 1,
    Month = 2,
}

impl From<u8> for AttestLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => AttestLevel::None,
            1 => AttestLevel::Week,
            2 => AttestLevel::Month,
            _ => AttestLevel::None,
        }
    }
}

impl From<i32> for AttestLevel {
    fn from(value: i32) -> Self {
        AttestLevel::from(value as u8)
    }
}

impl From<i64> for AttestLevel {
    fn from(value: i64) -> Self {
        AttestLevel::from(value as u8)
    }
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
    pub attest_level: AttestLevel,
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
            attest_level: AttestLevel::None,
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

    pub fn with_attest_level(mut self, attest_level: AttestLevel) -> Self {
        self.attest_level = attest_level;
        self
    }
}

/// Time tracking statistics for a period.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeInfo {
    pub period_time_left: f64,
    pub worked_period_time: f64,
    pub scheduled_period_time: f64,
    pub worked_period_with_absence_time: f64,
    pub flex_time_current: f64,
}

impl TimeInfo {
    pub fn new(
        period_time_left: f64,
        worked_period_time: f64,
        scheduled_period_time: f64,
        worked_period_with_absence_time: f64,
        flex_time_current: f64,
    ) -> Self {
        Self {
            period_time_left,
            worked_period_time,
            scheduled_period_time,
            worked_period_with_absence_time,
            flex_time_current,
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
    pub reg_day: String,
    pub week_number: i32,
    pub note: String,
}

/// Request to edit an existing time entry.
#[derive(Debug, Clone)]
pub struct EditTimeEntryRequest {
    pub registration_id: String,
    pub project_id: ProjectId,
    pub project_name: String,
    pub activity_id: ActivityId,
    pub activity_name: String,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub reg_day: String,
    pub week_number: i32,
    pub note: String,
    /// Original registration day if the date changed.
    pub original_reg_day: Option<String>,
}

/// A local timer history entry (stored in our database).
///
/// This tracks start/end times for time entries that may also exist
/// in the external time tracking provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerHistoryEntry {
    pub id: TimerHistoryId,
    pub user_id: UserId,
    pub source: TimerSource,
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
        source: TimerSource,
        start_time: OffsetDateTime,
        created_at: OffsetDateTime,
    ) -> Self {
        Self {
            id: id.into(),
            user_id: user_id.into(),
            source,
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
    pub source: TimerSource,
    pub registration_id: String,
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub project_id: Option<ProjectId>,
    pub project_name: Option<String>,
    pub activity_id: Option<ActivityId>,
    pub activity_name: Option<String>,
    pub note: String,
}
