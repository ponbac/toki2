use kleer::{
    KleerActivityReadable, KleerClientProjectReadable, KleerEventReadable, KleerPayrollEvent,
    KleerPayrollEventType, KleerScheduleMetadata, KleerStatusType,
};

use crate::domain::{
    models::{Activity, Project, ProjectId, TimeEntry, TimeEntryStatus},
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

pub fn to_domain_scheduled_hours(schedule: &[KleerScheduleMetadata]) -> f64 {
    schedule.iter().map(|day| day.actual_hours).sum()
}

pub fn to_domain_absence_hours(payroll_events: &[KleerPayrollEvent]) -> f64 {
    payroll_events
        .iter()
        .filter(|event| {
            !matches!(
                event.event_type,
                KleerPayrollEventType::WorkHour | KleerPayrollEventType::Unknown
            )
        })
        .map(|event| event.hours)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::WeeklyStats;

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

    #[test]
    fn weekly_stats_use_absence_as_covered_hours() {
        let stats = WeeklyStats::new(32.0, 40.0, 8.0);

        assert_eq!(stats.worked_hours, 32.0);
        assert_eq!(stats.absence_hours, 8.0);
        assert_eq!(stats.covered_hours, 40.0);
        assert_eq!(stats.remaining_hours, 0.0);
        assert_eq!(stats.period_flex_hours, 0.0);
    }

    #[test]
    fn weekly_stats_allow_positive_and_negative_period_flex() {
        let positive = WeeklyStats::new(42.0, 40.0, 0.0);
        let negative = WeeklyStats::new(30.0, 40.0, 0.0);

        assert_eq!(positive.remaining_hours, 0.0);
        assert_eq!(positive.period_flex_hours, 2.0);
        assert_eq!(negative.remaining_hours, 10.0);
        assert_eq!(negative.period_flex_hours, -10.0);
    }

    #[test]
    fn absence_hours_exclude_work_hour_payroll_events() {
        let events = vec![
            KleerPayrollEvent {
                id: Some(1),
                date: time::Date::from_calendar_date(2026, time::Month::April, 20).unwrap(),
                hours: 8.0,
                event_type: KleerPayrollEventType::Vacation,
                child: None,
                comment: Some(String::new()),
            },
            KleerPayrollEvent {
                id: Some(2),
                date: time::Date::from_calendar_date(2026, time::Month::April, 21).unwrap(),
                hours: 2.0,
                event_type: KleerPayrollEventType::WorkHour,
                child: None,
                comment: Some(String::new()),
            },
            KleerPayrollEvent {
                id: Some(3),
                date: time::Date::from_calendar_date(2026, time::Month::April, 22).unwrap(),
                hours: 4.0,
                event_type: KleerPayrollEventType::Sick,
                child: None,
                comment: Some(String::new()),
            },
            KleerPayrollEvent {
                id: Some(4),
                date: time::Date::from_calendar_date(2026, time::Month::April, 23).unwrap(),
                hours: 1.0,
                event_type: KleerPayrollEventType::Unknown,
                child: None,
                comment: Some(String::new()),
            },
        ];

        assert_eq!(to_domain_absence_hours(&events), 12.0);
    }
}
