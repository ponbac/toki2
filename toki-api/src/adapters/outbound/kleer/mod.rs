mod conversions;

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use kleer::{
    KleerActivityList, KleerClient, KleerClientProjectList, KleerClientProjectReadable,
    KleerCredentials, KleerError, KleerEventReadable, KleerEventRestrictionList,
    KleerEventWritable, KleerIdRef,
};
use time::Date;

use crate::domain::{
    models::{
        Activity, ActivityId, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId,
        TimeEntry, TimeEntryDayStatus, TimerId, WeeklyStats,
    },
    ports::outbound::TimeTrackingClient,
    TimeTrackingError,
};

use self::conversions::{
    to_domain_absence_hours, to_domain_activity, to_domain_project, to_domain_scheduled_hours,
    to_domain_status, to_domain_time_entry,
};

pub struct KleerAdapter {
    client: KleerClient,
    target_user_id: i64,
}

struct VerifiedKleerEventTarget {
    project_id: i64,
    activity_id: i64,
}

impl KleerAdapter {
    const MISSING_NOTE_COMMENT: &'static str = "missing note";

    pub fn new(
        credentials: KleerCredentials,
        target_user_id: i64,
    ) -> Result<Self, TimeTrackingError> {
        let client = KleerClient::new(credentials).map_err(map_kleer_error)?;
        Ok(Self {
            client,
            target_user_id,
        })
    }

    fn project_visible_to_user(project: &KleerClientProjectReadable, user_id: i64) -> bool {
        project.active
            && (project.all_users
                || project
                    .users
                    .iter()
                    .any(|assignment| assignment.user.id == user_id))
    }

    fn allowed_activity_ids(
        project: &KleerClientProjectReadable,
        user_id: i64,
        all_activity_ids: &HashSet<i64>,
    ) -> HashSet<i64> {
        if project.all_activities {
            return all_activity_ids.clone();
        }

        let project_activity_ids: HashSet<_> = project
            .activities
            .iter()
            .map(|assignment| assignment.activity.id)
            .collect();

        let user_activity_ids = project
            .users
            .iter()
            .find(|assignment| assignment.user.id == user_id)
            .and_then(|assignment| {
                if assignment.activities.is_empty() {
                    None
                } else {
                    Some(
                        assignment
                            .activities
                            .iter()
                            .map(|assignment| assignment.activity.id)
                            .collect(),
                    )
                }
            });

        match (project_activity_ids.is_empty(), user_activity_ids) {
            (true, Some(user_activity_ids)) => user_activity_ids,
            (_, None) => project_activity_ids,
            (false, Some(user_activity_ids)) => project_activity_ids
                .intersection(&user_activity_ids)
                .copied()
                .collect(),
        }
    }

    fn parse_kleer_id(raw: &str, label: &str) -> Result<i64, TimeTrackingError> {
        raw.parse::<i64>()
            .map_err(|_| TimeTrackingError::unknown(format!("invalid Kleer {label}: {raw}")))
    }

    fn visible_project(
        projects: &KleerClientProjectList,
        project_id: i64,
        user_id: i64,
    ) -> Result<&KleerClientProjectReadable, TimeTrackingError> {
        projects
            .client_project_readables
            .iter()
            .find(|project| {
                project.id.id == project_id && Self::project_visible_to_user(project, user_id)
            })
            .ok_or_else(|| TimeTrackingError::ProjectNotFound(project_id.to_string()))
    }

    fn all_activity_ids(activities: &KleerActivityList) -> HashSet<i64> {
        activities
            .activity_readables
            .iter()
            .map(|activity| activity.id.id)
            .collect()
    }

    fn ensure_activity_allowed(
        project: &KleerClientProjectReadable,
        activities: &KleerActivityList,
        user_id: i64,
        activity_id: i64,
    ) -> Result<(), TimeTrackingError> {
        let all_activity_ids = Self::all_activity_ids(activities);
        let allowed_activity_ids = Self::allowed_activity_ids(project, user_id, &all_activity_ids);

        if allowed_activity_ids.contains(&activity_id) {
            Ok(())
        } else {
            Err(TimeTrackingError::ActivityNotFound(activity_id.to_string()))
        }
    }

    async fn ensure_project_activity_allowed(
        &self,
        project_id: &ProjectId,
        activity_id: &ActivityId,
    ) -> Result<VerifiedKleerEventTarget, TimeTrackingError> {
        let project_id = Self::parse_kleer_id(project_id.as_str(), "project id")?;
        let activity_id = Self::parse_kleer_id(activity_id.as_str(), "activity id")?;
        let projects = self
            .client
            .list_active_client_projects()
            .await
            .map_err(map_kleer_error)?;
        let activities = self
            .client
            .list_activities()
            .await
            .map_err(map_kleer_error)?;

        let project = Self::visible_project(&projects, project_id, self.target_user_id)?;
        Self::ensure_activity_allowed(project, &activities, self.target_user_id, activity_id)?;

        Ok(VerifiedKleerEventTarget {
            project_id,
            activity_id,
        })
    }

    fn ensure_event_owned_by_target_user(
        &self,
        event: &KleerEventReadable,
    ) -> Result<(), TimeTrackingError> {
        if event.user.id == self.target_user_id {
            Ok(())
        } else {
            // Do not reveal whether another user's event exists.
            Err(TimeTrackingError::TimerNotFound)
        }
    }

    async fn ensure_event_owned(&self, event_id: i64) -> Result<(), TimeTrackingError> {
        let event = self
            .client
            .get_event(event_id)
            .await
            .map_err(map_kleer_error)?;

        self.ensure_event_owned_by_target_user(&event)
    }

    fn build_event_writable(
        target: VerifiedKleerEventTarget,
        start_time: time::OffsetDateTime,
        end_time: time::OffsetDateTime,
        note: &str,
        user_id: i64,
    ) -> KleerEventWritable {
        let note = Self::event_comment(note);

        KleerEventWritable {
            foreign_id: Self::event_foreign_id(
                user_id,
                target.project_id,
                target.activity_id,
                start_time,
            ),
            user: KleerIdRef { id: user_id },
            activity: KleerIdRef {
                id: target.activity_id,
            },
            client_project: Some(KleerIdRef {
                id: target.project_id,
            }),
            child: None,
            date: start_time.date(),
            hours: (end_time - start_time).whole_seconds() as f64 / 3600.0,
            comment: note.to_string(),
            internal_comment: Some(note.to_string()),
        }
    }

    fn event_comment(note: &str) -> &str {
        if note.trim().is_empty() {
            Self::MISSING_NOTE_COMMENT
        } else {
            note
        }
    }

    fn event_foreign_id(
        user_id: i64,
        project_id: i64,
        activity_id: i64,
        start_time: time::OffsetDateTime,
    ) -> String {
        // Kleer documents this as optional, but event creation returns a generic
        // 500 in the test environment unless a foreign id is present.
        format!(
            "toki-{user_id}-{project_id}-{activity_id}-{}",
            start_time.unix_timestamp_nanos()
        )
    }

    fn to_domain_day_statuses(statuses: KleerEventRestrictionList) -> Vec<TimeEntryDayStatus> {
        statuses
            .event_restriction_readables
            .into_iter()
            .filter_map(|restriction| {
                restriction
                    .status
                    .event_date
                    .map(|date| TimeEntryDayStatus {
                        date,
                        status: to_domain_status(restriction.status.status_type),
                    })
            })
            .collect()
    }
}

#[async_trait]
impl TimeTrackingClient for KleerAdapter {
    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        let projects = self
            .client
            .list_active_client_projects()
            .await
            .map_err(map_kleer_error)?;

        Ok(projects
            .client_project_readables
            .iter()
            .filter(|project| Self::project_visible_to_user(project, self.target_user_id))
            .map(to_domain_project)
            .collect())
    }

    async fn get_activities(
        &self,
        project_id: &ProjectId,
        _date_range: (Date, Date),
    ) -> Result<Vec<Activity>, TimeTrackingError> {
        let projects = self
            .client
            .list_active_client_projects()
            .await
            .map_err(map_kleer_error)?;
        let activities = self
            .client
            .list_activities()
            .await
            .map_err(map_kleer_error)?;

        let project_id_value = Self::parse_kleer_id(project_id.as_str(), "project id")?;
        let project = Self::visible_project(&projects, project_id_value, self.target_user_id)?;
        let all_activity_ids = Self::all_activity_ids(&activities);
        let allowed_activity_ids =
            Self::allowed_activity_ids(project, self.target_user_id, &all_activity_ids);

        Ok(activities
            .activity_readables
            .iter()
            .filter(|activity| allowed_activity_ids.contains(&activity.id.id))
            .map(|activity| to_domain_activity(activity, project_id))
            .collect())
    }

    async fn get_time_info(
        &self,
        date_range: (Date, Date),
    ) -> Result<WeeklyStats, TimeTrackingError> {
        let events = self
            .client
            .list_events(self.target_user_id, date_range.0, date_range.1)
            .await
            .map_err(map_kleer_error)?;
        let schedule = self
            .client
            .list_schedule_summary(self.target_user_id, date_range.0, date_range.1)
            .await
            .or_else(empty_schedule_for_missing_payroll_user)?;
        let payroll_events = self
            .client
            .list_payroll_events(self.target_user_id, date_range.0, date_range.1)
            .await
            .or_else(empty_payroll_events_for_missing_payroll_user)?;

        let worked_hours: f64 = events
            .event_readables
            .into_iter()
            .filter(|event| event.client_project.is_some())
            .map(|event| event.hours)
            .sum();
        let scheduled_hours = to_domain_scheduled_hours(&schedule.payroll_user_schedule_metadatas);
        let absence_hours = to_domain_absence_hours(&payroll_events.payroll_events);

        Ok(WeeklyStats::new(
            worked_hours,
            scheduled_hours,
            absence_hours,
        ))
    }

    async fn get_time_entries(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntry>, TimeTrackingError> {
        let projects = self
            .client
            .list_client_projects()
            .await
            .map_err(map_kleer_error)?;
        let activities = self
            .client
            .list_activities()
            .await
            .map_err(map_kleer_error)?;
        let events = self
            .client
            .list_events(self.target_user_id, date_range.0, date_range.1)
            .await
            .map_err(map_kleer_error)?;
        let statuses = self
            .client
            .list_event_statuses(self.target_user_id, date_range.0, date_range.1)
            .await
            .map_err(map_kleer_error)?;

        let project_names: HashMap<_, _> = projects
            .client_project_readables
            .iter()
            .map(|project| (project.id.id, project.name.clone()))
            .collect();
        let activity_names: HashMap<_, _> = activities
            .activity_readables
            .iter()
            .map(|activity| (activity.id.id, activity.name.clone()))
            .collect();
        let status_by_date: HashMap<_, _> = Self::to_domain_day_statuses(statuses)
            .into_iter()
            .map(|day_status| (day_status.date, day_status.status))
            .collect();

        let mut entries = Vec::new();
        for event in events
            .event_readables
            .iter()
            .filter(|event| event.client_project.is_some())
        {
            let Some(project_id) = event.client_project.as_ref().map(|project| project.id) else {
                continue;
            };

            let Some(project_name) = project_names.get(&project_id) else {
                tracing::warn!(
                    "skipping Kleer event {}: missing project lookup",
                    event.id.id
                );
                continue;
            };
            let Some(activity_name) = activity_names.get(&event.activity.id) else {
                tracing::warn!(
                    "skipping Kleer event {}: missing activity lookup",
                    event.id.id
                );
                continue;
            };

            let status = event
                .status
                .as_ref()
                .map(|status| to_domain_status(status.status_type.clone()))
                .or_else(|| status_by_date.get(&event.date).copied())
                .unwrap_or_default();

            entries.push(to_domain_time_entry(
                event,
                project_name.clone(),
                activity_name.clone(),
                status,
            )?);
        }

        Ok(entries)
    }

    async fn get_time_entry_day_statuses(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntryDayStatus>, TimeTrackingError> {
        let statuses = self
            .client
            .list_event_statuses(self.target_user_id, date_range.0, date_range.1)
            .await
            .map_err(map_kleer_error)?;

        Ok(Self::to_domain_day_statuses(statuses))
    }

    async fn create_time_entry(
        &self,
        request: &CreateTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        let target = self
            .ensure_project_activity_allowed(&request.project_id, &request.activity_id)
            .await?;

        let payload = Self::build_event_writable(
            target,
            request.start_time,
            request.end_time,
            &request.note,
            self.target_user_id,
        );
        let saved = self
            .client
            .create_event(&payload)
            .await
            .map_err(map_kleer_error)?;

        Ok(TimerId::new(saved.id.to_string()))
    }

    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        let event_id = Self::parse_kleer_id(&request.registration_id, "event id")?;
        self.ensure_event_owned(event_id).await?;
        let target = self
            .ensure_project_activity_allowed(&request.project_id, &request.activity_id)
            .await?;

        let payload = Self::build_event_writable(
            target,
            request.start_time,
            request.end_time,
            &request.note,
            self.target_user_id,
        );
        let saved = self
            .client
            .update_event(event_id, &payload)
            .await
            .map_err(map_kleer_error)?;

        Ok(TimerId::new(saved.id.to_string()))
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        let event_id = Self::parse_kleer_id(registration_id, "event id")?;
        self.ensure_event_owned(event_id).await?;

        self.client
            .delete_event(event_id)
            .await
            .map_err(map_kleer_error)?;
        Ok(())
    }
}

fn map_kleer_error(error: KleerError) -> TimeTrackingError {
    match error {
        KleerError::NotFound => TimeTrackingError::TimerNotFound,
        KleerError::Unauthorized => {
            TimeTrackingError::unknown("Kleer integration token is invalid or expired")
        }
        KleerError::Forbidden => TimeTrackingError::unknown("Kleer access forbidden"),
        KleerError::InvalidConfig(message)
        | KleerError::Request(message)
        | KleerError::Deserialize { message, .. } => TimeTrackingError::unknown(message),
        KleerError::Response { status, body } => {
            let message = kleer_response_message(&body);
            tracing::warn!("Kleer returned non-success response: status={status}, body={message}");
            TimeTrackingError::unknown(format!("Kleer returned {status}: {message}"))
        }
    }
}

fn kleer_response_message(body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| "empty response body".to_string());

    let message = message.replace(['\r', '\n', '\t'], " ");
    message.chars().take(500).collect()
}

fn empty_schedule_for_missing_payroll_user(
    error: KleerError,
) -> Result<kleer::KleerScheduleMetadataList, TimeTrackingError> {
    if is_missing_payroll_user(&error) {
        Ok(Default::default())
    } else {
        Err(map_kleer_error(error))
    }
}

fn empty_payroll_events_for_missing_payroll_user(
    error: KleerError,
) -> Result<kleer::KleerPayrollEventList, TimeTrackingError> {
    if is_missing_payroll_user(&error) {
        Ok(Default::default())
    } else {
        Err(map_kleer_error(error))
    }
}

fn is_missing_payroll_user(error: &KleerError) -> bool {
    matches!(
        error,
        KleerError::Response { status, body }
            if *status == reqwest::StatusCode::INTERNAL_SERVER_ERROR
                && body.contains("PayrollUserDoesNotExistException")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kleer::{KleerProjectActivityAssignment, KleerProjectUserAssignment};
    use time::Month;

    const TARGET_USER_ID: i64 = 987;

    fn activity_assignment(activity_id: i64) -> KleerProjectActivityAssignment {
        KleerProjectActivityAssignment {
            activity: KleerIdRef { id: activity_id },
        }
    }

    fn user_assignment(
        user_id: i64,
        activity_ids: impl IntoIterator<Item = i64>,
    ) -> KleerProjectUserAssignment {
        KleerProjectUserAssignment {
            user: KleerIdRef { id: user_id },
            activities: activity_ids.into_iter().map(activity_assignment).collect(),
        }
    }

    fn project(
        all_activities: bool,
        project_activity_ids: impl IntoIterator<Item = i64>,
        users: Vec<KleerProjectUserAssignment>,
    ) -> KleerClientProjectReadable {
        KleerClientProjectReadable {
            id: KleerIdRef { id: 123 },
            number: "P-123".to_string(),
            name: "Project".to_string(),
            active: true,
            all_activities,
            activities: project_activity_ids
                .into_iter()
                .map(activity_assignment)
                .collect(),
            all_users: false,
            users,
        }
    }

    fn activity_ids(ids: impl IntoIterator<Item = i64>) -> HashSet<i64> {
        ids.into_iter().collect()
    }

    #[test]
    fn all_project_activities_are_not_limited_by_user_assignment_activities() {
        let all_activity_ids = activity_ids([10, 20, 30]);
        let project = project(true, [], vec![user_assignment(TARGET_USER_ID, [10])]);

        let allowed =
            KleerAdapter::allowed_activity_ids(&project, TARGET_USER_ID, &all_activity_ids);

        assert_eq!(allowed, all_activity_ids);
    }

    #[test]
    fn explicit_project_activities_are_limited_by_user_assignment_activities() {
        let all_activity_ids = activity_ids([10, 20, 30]);
        let project = project(
            false,
            [10, 20, 30],
            vec![user_assignment(TARGET_USER_ID, [10, 30])],
        );

        let allowed =
            KleerAdapter::allowed_activity_ids(&project, TARGET_USER_ID, &all_activity_ids);

        assert_eq!(allowed, activity_ids([10, 30]));
    }

    #[test]
    fn user_activity_assignment_allows_activities_when_project_activity_list_is_empty() {
        let all_activity_ids = activity_ids([10, 20, 30]);
        let project = project(false, [], vec![user_assignment(TARGET_USER_ID, [20])]);

        let allowed =
            KleerAdapter::allowed_activity_ids(&project, TARGET_USER_ID, &all_activity_ids);

        assert_eq!(allowed, activity_ids([20]));
    }

    #[test]
    fn explicit_project_activities_allow_all_project_activities_without_user_activity_restrictions()
    {
        let all_activity_ids = activity_ids([10, 20, 30]);
        let project = project(false, [10, 20], vec![user_assignment(TARGET_USER_ID, [])]);

        let allowed =
            KleerAdapter::allowed_activity_ids(&project, TARGET_USER_ID, &all_activity_ids);

        assert_eq!(allowed, activity_ids([10, 20]));
    }

    #[test]
    fn event_payload_saves_note_as_external_and_internal_comment() {
        let start_time = Date::from_calendar_date(2026, Month::May, 6)
            .unwrap()
            .with_hms(8, 0, 0)
            .unwrap()
            .assume_utc();
        let end_time = start_time + time::Duration::hours(2);

        let payload = KleerAdapter::build_event_writable(
            VerifiedKleerEventTarget {
                project_id: 321,
                activity_id: 654,
            },
            start_time,
            end_time,
            "Worked on PR review",
            987,
        );

        assert_eq!(payload.comment, "Worked on PR review");
        assert_eq!(
            payload.internal_comment.as_deref(),
            Some("Worked on PR review")
        );
    }

    #[test]
    fn event_payload_replaces_empty_note_with_missing_note() {
        let start_time = Date::from_calendar_date(2026, Month::May, 6)
            .unwrap()
            .with_hms(8, 0, 0)
            .unwrap()
            .assume_utc();
        let end_time = start_time + time::Duration::hours(2);

        for note in ["", "   ", "\n\t"] {
            let payload = KleerAdapter::build_event_writable(
                VerifiedKleerEventTarget {
                    project_id: 321,
                    activity_id: 654,
                },
                start_time,
                end_time,
                note,
                987,
            );

            assert_eq!(payload.comment, KleerAdapter::MISSING_NOTE_COMMENT);
            assert_eq!(
                payload.internal_comment.as_deref(),
                Some(KleerAdapter::MISSING_NOTE_COMMENT)
            );
        }
    }
}
