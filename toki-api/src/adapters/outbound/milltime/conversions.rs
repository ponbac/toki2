use chrono::Datelike;

use crate::domain::{
    models::{Activity, AttestLevel, Project, ProjectId, TimeEntry, TimeInfo},
    TimeTrackingError,
};

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
pub fn to_domain_time_entry(entry: milltime::TimeEntry) -> Result<TimeEntry, TimeTrackingError> {
    // Convert chrono::NaiveDate to time::Date
    let date = time::Date::from_calendar_date(
        entry.date.year(),
        time::Month::try_from(entry.date.month() as u8).map_err(|_| {
            TimeTrackingError::unknown(format!("Invalid month: {}", entry.date.month()))
        })?,
        entry.date.day() as u8,
    )
    .map_err(|e| TimeTrackingError::unknown(format!("Invalid date {}: {}", entry.date, e)))?;

    // Get week number from the date
    let week_number = entry.date.iso_week().week() as u8;

    Ok(TimeEntry::new(
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
    .with_attest_level(to_domain_attest_level(entry.attest_level)))
}

/// Convert Milltime AttestLevel to domain AttestLevel.
pub fn to_domain_attest_level(level: milltime::AttestLevel) -> AttestLevel {
    match level {
        milltime::AttestLevel::None => AttestLevel::None,
        milltime::AttestLevel::Week => AttestLevel::Week,
        milltime::AttestLevel::Month => AttestLevel::Month,
    }
}
