use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::domain::models::{
    AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, ActiveTimer, Activity, Project,
    TimeEntry, TimeEntryDayStatus, TimeEntryStatus, TimerHistoryEntry, WeeklyStats,
};

/// Response for the get timer endpoint.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTimerResponse {
    pub timer: Option<TimerResponse>,
}

/// Response for saving the active timer.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerResponse {
    pub entry: TimeEntryResponse,
}

/// Active timer response - all timers are standalone now.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimerResponse {
    /// When the timer was started (ISO 8601).
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub start_time: OffsetDateTime,
    /// Project ID (if set).
    pub project_id: Option<String>,
    /// Project name (if set).
    pub project_name: Option<String>,
    /// Activity ID/code (if set).
    pub activity_id: Option<String>,
    /// Activity name (if set).
    pub activity_name: Option<String>,
    /// User note.
    pub note: String,
    /// Elapsed hours.
    pub hours: i64,
    /// Elapsed minutes (within current hour).
    pub minutes: i64,
    /// Elapsed seconds (within current minute).
    pub seconds: i64,
}

impl From<ActiveTimer> for TimerResponse {
    fn from(timer: ActiveTimer) -> Self {
        let (hours, minutes, seconds) = timer.elapsed_hms();
        Self {
            start_time: timer.started_at,
            project_id: timer.project_id.map(|id| id.to_string()),
            project_name: timer.project_name,
            activity_id: timer.activity_id.map(|id| id.to_string()),
            activity_name: timer.activity_name,
            note: timer.note,
            hours,
            minutes,
            seconds,
        }
    }
}

/// Project response - simplified for frontend use.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResponse {
    pub project_id: String,
    pub project_name: String,
}

impl From<Project> for ProjectResponse {
    fn from(project: Project) -> Self {
        Self {
            project_id: project.id.to_string(),
            project_name: project.name,
        }
    }
}

/// Activity response - simplified for frontend use.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityResponse {
    /// Activity code (used in API calls).
    pub activity: String,
    /// Activity display name.
    pub activity_name: String,
}

impl From<Activity> for ActivityResponse {
    fn from(activity: Activity) -> Self {
        Self {
            activity: activity.id.to_string(),
            activity_name: activity.name,
        }
    }
}

/// Time entry response - completed time registration.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimeEntryStatusResponse {
    Open,
    Approved,
    Certified,
}

impl From<TimeEntryStatus> for TimeEntryStatusResponse {
    fn from(status: TimeEntryStatus) -> Self {
        match status {
            TimeEntryStatus::Open => Self::Open,
            TimeEntryStatus::Approved => Self::Approved,
            TimeEntryStatus::Certified => Self::Certified,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryResponse {
    pub registration_id: String,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    /// Date in YYYY-MM-DD format.
    pub date: String,
    pub hours: f64,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub end_time: Option<OffsetDateTime>,
    pub week_number: u8,
    /// Attestation status: `open`, `approved`, or `certified`.
    pub status: TimeEntryStatusResponse,
}

impl From<TimeEntry> for TimeEntryResponse {
    fn from(entry: TimeEntry) -> Self {
        Self {
            registration_id: entry.registration_id,
            project_id: entry.project_id.to_string(),
            project_name: entry.project_name,
            activity_id: entry.activity_id.to_string(),
            activity_name: entry.activity_name,
            date: entry.date.to_string(),
            hours: entry.hours,
            note: entry.note,
            start_time: entry.start_time,
            end_time: entry.end_time,
            week_number: entry.week_number,
            status: entry.status.into(),
        }
    }
}

/// Date-level time entry status response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryDayStatusResponse {
    /// Date in YYYY-MM-DD format.
    pub date: String,
    /// Attestation status: `open`, `approved`, or `certified`.
    pub status: TimeEntryStatusResponse,
}

impl From<TimeEntryDayStatus> for TimeEntryDayStatusResponse {
    fn from(day_status: TimeEntryDayStatus) -> Self {
        Self {
            date: day_status.date.to_string(),
            status: day_status.status.into(),
        }
    }
}

/// Timer history entry response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimerHistoryEntryResponse {
    pub id: i32,
    pub registration_id: Option<String>,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub created_at: OffsetDateTime,
}

impl From<TimerHistoryEntry> for TimerHistoryEntryResponse {
    fn from(entry: TimerHistoryEntry) -> Self {
        Self {
            id: entry.id.as_i32(),
            registration_id: entry.registration_id,
            user_id: entry.user_id.as_i32(),
            start_time: entry.start_time,
            end_time: entry.end_time,
            project_id: entry.project_id.map(|p| p.to_string()),
            project_name: entry.project_name,
            activity_id: entry.activity_id.map(|a| a.to_string()),
            activity_name: entry.activity_name,
            note: entry.note,
            created_at: entry.created_at,
        }
    }
}

/// Weekly stats response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyStatsResponse {
    pub worked_hours: f64,
    pub scheduled_hours: f64,
    pub remaining_hours: f64,
    pub absence_hours: f64,
    pub covered_hours: f64,
    pub period_flex_hours: f64,
}

impl From<WeeklyStats> for WeeklyStatsResponse {
    fn from(info: WeeklyStats) -> Self {
        Self {
            worked_hours: info.worked_hours,
            scheduled_hours: info.scheduled_hours,
            remaining_hours: info.remaining_hours,
            absence_hours: info.absence_hours,
            covered_hours: info.covered_hours,
            period_flex_hours: info.period_flex_hours,
        }
    }
}

/// Absence entry response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceEntryResponse {
    pub absence_id: String,
    pub date: String,
    pub hours: f64,
    pub absence_type: AbsenceType,
    pub absence_type_label: &'static str,
    pub child: Option<String>,
    pub comment: Option<String>,
    pub managed: bool,
    pub deletable: bool,
}

/// Available absence type response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceTypeResponse {
    pub absence_type: AbsenceType,
    pub absence_type_label: &'static str,
}

/// Registered child available for child-related absence reporting.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceChildResponse {
    pub name: String,
    pub birth_date: Option<String>,
}

impl From<AbsenceType> for AbsenceTypeResponse {
    fn from(absence_type: AbsenceType) -> Self {
        Self {
            absence_type,
            absence_type_label: absence_type.label(),
        }
    }
}

impl From<AbsenceChild> for AbsenceChildResponse {
    fn from(child: AbsenceChild) -> Self {
        Self {
            name: child.name,
            birth_date: child.birth_date.map(|date| date.to_string()),
        }
    }
}

impl From<AbsenceEntry> for AbsenceEntryResponse {
    fn from(entry: AbsenceEntry) -> Self {
        Self {
            absence_id: entry.absence_id,
            date: entry.date.to_string(),
            hours: entry.hours,
            absence_type: entry.absence_type,
            absence_type_label: entry.absence_type.label(),
            child: entry.child,
            comment: entry.comment,
            managed: entry.managed,
            deletable: entry.deletable,
        }
    }
}

/// Default hours for one absence day.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceDayDefaultResponse {
    pub date: String,
    pub scheduled_hours: f64,
}

impl From<AbsenceDayDefault> for AbsenceDayDefaultResponse {
    fn from(day: AbsenceDayDefault) -> Self {
        Self {
            date: day.date.to_string(),
            scheduled_hours: day.scheduled_hours,
        }
    }
}
