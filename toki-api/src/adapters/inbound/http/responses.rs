//! HTTP response types for time tracking endpoints.
//!
//! These types serialize to the JSON format expected by the frontend.

use serde::Serialize;
use time::OffsetDateTime;

use crate::domain::models::{ActiveTimer, Activity, AttestLevel, Project, TimeEntry, TimeInfo};

/// Response for the get timer endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimerResponse {
    pub timer: Option<TimerResponse>,
}

/// Active timer response - all timers are standalone now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerResponse {
    /// When the timer was started (ISO 8601).
    #[serde(with = "time::serde::rfc3339")]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    pub week_number: u8,
    pub attest_level: AttestLevel,
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
            attest_level: entry.attest_level,
        }
    }
}

/// Time info response - period statistics.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeInfoResponse {
    pub period_time_left: f64,
    pub worked_period_time: f64,
    pub scheduled_period_time: f64,
    pub worked_period_with_absence_time: f64,
    pub flex_time_current: f64,
}

impl From<TimeInfo> for TimeInfoResponse {
    fn from(info: TimeInfo) -> Self {
        Self {
            period_time_left: info.period_time_left,
            worked_period_time: info.worked_period_time,
            scheduled_period_time: info.scheduled_period_time,
            worked_period_with_absence_time: info.worked_period_with_absence_time,
            flex_time_current: info.flex_time_current,
        }
    }
}
