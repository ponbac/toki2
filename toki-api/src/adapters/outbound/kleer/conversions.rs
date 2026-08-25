use kleer::{
    KleerActivityReadable, KleerClientProjectReadable, KleerEventReadable, KleerPayrollChild,
    KleerScheduleMetadata, KleerStatusType,
};

use crate::domain::{
    models::{
        AbsenceChild, AbsenceEntry, AbsenceType, Activity, Project, ProjectId, TimeEntry,
        TimeEntryStatus,
    },
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
    let project_id = event.client_project.as_ref().ok_or_else(|| {
        TimeTrackingError::provider_unavailable("missing client project on Kleer event")
    })?;

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

pub fn to_domain_absence_child(child: KleerPayrollChild) -> AbsenceChild {
    AbsenceChild {
        name: child.name,
        birth_date: child.birth_date,
    }
}

pub fn to_domain_absence_entry_from_event(
    event: &KleerEventReadable,
    absence_type: AbsenceType,
) -> Option<AbsenceEntry> {
    if event.client_project.is_some() {
        return None;
    }
    Some(AbsenceEntry {
        absence_id: event.id.id.to_string(),
        date: event.date,
        hours: event.hours,
        absence_type,
        child: event.child.clone().filter(|value| !value.trim().is_empty()),
        comment: event
            .comment
            .clone()
            .filter(|value| !value.trim().is_empty()),
        managed: true,
        deletable: true,
    })
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
}
