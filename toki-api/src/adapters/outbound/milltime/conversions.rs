use chrono::Datelike;
use time::OffsetDateTime;

use crate::domain::models::{Activity, ActiveTimer, AttestLevel, Project, ProjectId, TimeEntry, TimeInfo, TimerSource};

/// Convert a Milltime TimerRegistration to a domain ActiveTimer.
pub fn to_domain_active_timer(timer: milltime::TimerRegistration) -> ActiveTimer {
    let started_at = parse_milltime_datetime(&timer.start_time)
        .unwrap_or_else(|| OffsetDateTime::now_utc());

    // All timers are now Standalone type
    let mut active_timer = ActiveTimer::new(TimerSource::Standalone, started_at)
        .with_note(timer.user_note);

    if !timer.project_id.is_empty() {
        active_timer = active_timer.with_project(timer.project_id.clone(), timer.project_name.clone());
    }

    if !timer.activity.is_empty() {
        active_timer = active_timer.with_activity(timer.activity.clone(), timer.activity_name.clone());
    }

    active_timer
}

/// Convert a Milltime ProjectSearchItem to a domain Project.
pub fn to_domain_project(item: milltime::ProjectSearchItem) -> Project {
    let code = match &item.project_nr {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };

    let mut project = Project::new(item.project_id, item.project_name);
    if let Some(code) = code {
        project = project.with_code(code);
    }
    project
}

/// Convert a Milltime Activity to a domain Activity.
pub fn to_domain_activity(activity: milltime::Activity, project_id: &ProjectId) -> Activity {
    Activity::new(
        activity.activity,
        activity.activity_name,
        project_id.clone(),
    )
}

/// Parse a Milltime datetime string to OffsetDateTime.
/// Milltime uses format like "2024-01-15 09:30:00".
fn parse_milltime_datetime(datetime_str: &str) -> Option<OffsetDateTime> {
    // Try RFC3339 first
    if let Ok(dt) = OffsetDateTime::parse(datetime_str, &time::format_description::well_known::Rfc3339) {
        return Some(dt);
    }

    // Try parsing as naive datetime and assume UTC
    let format = time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").ok()?;
    if let Ok(parsed) = time::PrimitiveDateTime::parse(datetime_str, &format) {
        return Some(parsed.assume_utc());
    }

    None
}

/// Convert a Milltime TimeInfo to a domain TimeInfo.
pub fn to_domain_time_info(info: milltime::TimeInfo) -> TimeInfo {
    TimeInfo::new(
        info.period_time_left,
        info.worked_period_time,
        info.scheduled_period_time,
        info.worked_period_with_absence_time,
        info.flex_time_current,
    )
}

/// Convert a Milltime TimeEntry to a domain TimeEntry.
pub fn to_domain_time_entry(entry: milltime::TimeEntry) -> TimeEntry {
    // Convert chrono::NaiveDate to time::Date
    let date = time::Date::from_calendar_date(
        entry.date.year(),
        time::Month::try_from(entry.date.month() as u8).unwrap_or(time::Month::January),
        entry.date.day() as u8,
    )
    .unwrap_or(time::Date::from_calendar_date(1970, time::Month::January, 1).unwrap());

    // Get week number from the date
    let week_number = entry.date.iso_week().week() as u8;

    TimeEntry::new(
        entry.registration_id,
        entry.project_id,
        entry.project_name,
        entry.activity_id,
        entry.activity_name,
        date,
        entry.hours,
    )
    .with_note(entry.note.unwrap_or_default())
    .with_week_number(week_number)
    .with_attest_level(to_domain_attest_level(entry.attest_level))
}

/// Convert Milltime AttestLevel to domain AttestLevel.
pub fn to_domain_attest_level(level: milltime::AttestLevel) -> AttestLevel {
    match level {
        milltime::AttestLevel::None => AttestLevel::None,
        milltime::AttestLevel::Week => AttestLevel::Week,
        milltime::AttestLevel::Month => AttestLevel::Month,
    }
}
