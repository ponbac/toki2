use kleer::{
    KleerActivityReadable, KleerClientProjectReadable, KleerEventReadable, KleerScheduleMetadata,
    KleerStatusType,
};

use crate::domain::{
    models::{Activity, Project, ProjectId, TimeEntry, TimeEntryStatus, WeeklyStats},
    TimeTrackingError,
};

pub fn to_domain_project(project: &KleerClientProjectReadable) -> Project {
    let mut domain = Project::new(project.id.id.to_string(), project.name.clone());
    if !project.number.is_empty() {
        domain = domain.with_code(project.number.clone());
    }
    domain
}

pub fn to_domain_activity(activity: &KleerActivityReadable, project_id: &ProjectId) -> Activity {
    Activity::new(
        activity.id.id.to_string(),
        activity.name.clone(),
        project_id.clone(),
    )
}

pub fn to_domain_status(status: KleerStatusType) -> TimeEntryStatus {
    match status {
        KleerStatusType::Open => TimeEntryStatus::Open,
        KleerStatusType::Approved => TimeEntryStatus::Approved,
        KleerStatusType::Certified => TimeEntryStatus::Certified,
    }
}

pub fn to_domain_time_entry(
    event: &KleerEventReadable,
    project_name: String,
    activity_name: String,
    status: TimeEntryStatus,
) -> Result<TimeEntry, TimeTrackingError> {
    let project_id = event
        .client_project
        .as_ref()
        .ok_or_else(|| TimeTrackingError::unknown("missing client project on event"))?;

    let note = event
        .comment
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            event
                .internal_comment
                .clone()
                .filter(|value| !value.trim().is_empty())
        });

    let entry = TimeEntry::new(
        event.id.id.to_string(),
        project_id.id.to_string(),
        project_name,
        event.activity.id.to_string(),
        activity_name,
        event.date,
        event.hours,
    )
    .with_week_number(event.date.iso_week())
    .with_status(status);

    Ok(match note {
        Some(note) => entry.with_note(note),
        None => entry,
    })
}

pub fn to_domain_weekly_stats(worked_hours: f64, scheduled_hours: f64) -> WeeklyStats {
    WeeklyStats::new(
        worked_hours,
        scheduled_hours,
        (scheduled_hours - worked_hours).max(0.0),
    )
}

pub fn to_domain_scheduled_hours(schedule: &[KleerScheduleMetadata]) -> f64 {
    schedule.iter().map(|day| day.actual_hours).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_hours_use_actual_hours() {
        let schedule = vec![
            KleerScheduleMetadata {
                date: time::Date::from_calendar_date(2026, time::Month::April, 20).unwrap(),
                level_of_employment: 1.0,
                gross_hours: 8.0,
                net_hours: 8.0,
                actual_hours: 8.0,
            },
            KleerScheduleMetadata {
                date: time::Date::from_calendar_date(2026, time::Month::April, 21).unwrap(),
                level_of_employment: 1.0,
                gross_hours: 8.0,
                net_hours: 8.0,
                actual_hours: 0.0,
            },
        ];

        assert_eq!(to_domain_scheduled_hours(&schedule), 8.0);
    }
}
