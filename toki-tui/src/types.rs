use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A project available for time tracking, derived from timer history.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// An activity belonging to a project.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Activity {
    pub id: String,
    pub name: String,
    pub project_id: String,
}

/// A timer history entry as returned by toki-api.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerHistoryEntry {
    pub id: i32,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    pub registration_id: Option<String>,
}

/// A completed time entry from Milltime (via GET /time-tracking/time-entries).
/// start_time / end_time are optional — present only if a local timer history record exists.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub registration_id: String,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    /// Date in YYYY-MM-DD format, e.g. "2026-02-24"
    pub date: String,
    pub hours: f64,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    #[allow(dead_code)]
    pub week_number: u8,
}

/// The current user, as returned by GET /me.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: i32,
    pub email: String,
    pub full_name: String,
}

/// Active timer as returned by GET /time-tracking/timer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTimerState {
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: OffsetDateTime,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: String,
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
}

/// Wrapper returned by GET /time-tracking/timer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimerResponse {
    pub timer: Option<ActiveTimerState>,
}

/// Time info returned by GET /time-tracking/time-info.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct TimeInfo {
    pub period_time_left: f64,
    pub worked_period_time: f64,
    pub scheduled_period_time: f64,
    pub worked_period_with_absence_time: f64,
    pub flex_time_current: f64,
}
