mod conversions;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration as CacheDuration,
};

use async_trait::async_trait;
use kleer::{
    KleerActivityList, KleerActivityReadable, KleerClient, KleerClientProjectList,
    KleerClientProjectReadable, KleerCredentials, KleerError, KleerEventReadable,
    KleerEventRestrictionList, KleerEventWritable, KleerIdRef,
};
use moka::future::Cache;
use time::{Date, Duration};

use crate::domain::{
    models::{
        AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, Activity, ActivityId,
        CreateAbsencesRequest, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId,
        TimeEntry, TimeEntryDayStatus, TimerId, WeeklyStats,
    },
    ports::outbound::TimeTrackingClient,
    TimeTrackingError,
};

use self::conversions::{
    to_domain_absence_child, to_domain_absence_entry_from_event, to_domain_activity,
    to_domain_project, to_domain_scheduled_hours, to_domain_status, to_domain_time_entry,
};

pub struct KleerAdapter {
    client: KleerClient,
    target_user_id: i64,
    metadata_cache: Arc<KleerMetadataCache>,
    metadata_cache_key: KleerMetadataCacheKey,
}

#[derive(Clone)]
pub struct KleerMetadataCache {
    client_projects: Cache<KleerMetadataCacheKey, KleerClientProjectList>,
    active_client_projects: Cache<KleerMetadataCacheKey, KleerClientProjectList>,
    activities: Cache<KleerMetadataCacheKey, KleerActivityList>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct KleerMetadataCacheKey {
    base_url: String,
    company_id: String,
}

struct VerifiedKleerEventTarget {
    project_id: i64,
    activity_id: i64,
}

#[derive(Debug, Clone)]
struct AbsenceActivityMap {
    by_type: HashMap<AbsenceType, i64>,
    by_activity_id: HashMap<i64, AbsenceType>,
}

const KLEER_ABSENCE_ACTIVITY_NAMES: &[(AbsenceType, &str)] = &[
    (AbsenceType::PaternityLeave, "10 dagar vid barns födelse"),
    (AbsenceType::ParentalLeave, "Föräldraledighet"),
    (AbsenceType::Furlough, "Permission"),
    (AbsenceType::Vacation, "Semester"),
    (AbsenceType::Sick, "Sjuk"),
    (AbsenceType::LeaveOfAbsence, "Tjänstledig"),
    (
        AbsenceType::LeaveOfAbsenceVacationEarned,
        "Tjänstledig (Semestergrundande)",
    ),
    (AbsenceType::Childcare, "VAB"),
    (AbsenceType::CloseRelativeCare, "Vård av nära anhörig"),
    (AbsenceType::OtherLeave, "Övrig frånvaro"),
    (
        AbsenceType::OtherLeaveVacationNotEarned,
        "Övrig frånvaro (Semestergrundande)",
    ),
];
const KLEER_METADATA_CACHE_TTL: CacheDuration = CacheDuration::from_secs(5 * 60);
const KLEER_METADATA_CACHE_MAX_ENTRIES: u64 = 32;

impl KleerMetadataCache {
    pub fn new() -> Self {
        Self {
            client_projects: metadata_cache(),
            active_client_projects: metadata_cache(),
            activities: metadata_cache(),
        }
    }
}

impl Default for KleerMetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

impl KleerMetadataCacheKey {
    fn from_credentials(credentials: &KleerCredentials) -> Self {
        Self {
            base_url: credentials.base_url.clone(),
            company_id: credentials.company_id.clone(),
        }
    }
}

fn metadata_cache<V>() -> Cache<KleerMetadataCacheKey, V>
where
    V: Clone + Send + Sync + 'static,
{
    Cache::builder()
        .time_to_live(KLEER_METADATA_CACHE_TTL)
        .max_capacity(KLEER_METADATA_CACHE_MAX_ENTRIES)
        .build()
}

fn cached_load_error(error: Arc<TimeTrackingError>) -> TimeTrackingError {
    (*error).clone()
}

impl KleerAdapter {
    const MISSING_NOTE_COMMENT: &'static str = "missing note";

    pub fn with_metadata_cache(
        credentials: KleerCredentials,
        target_user_id: i64,
        metadata_cache: Arc<KleerMetadataCache>,
    ) -> Result<Self, TimeTrackingError> {
        let metadata_cache_key = KleerMetadataCacheKey::from_credentials(&credentials);
        let client = KleerClient::new(credentials).map_err(map_kleer_error)?;
        Ok(Self {
            client,
            target_user_id,
            metadata_cache,
            metadata_cache_key,
        })
    }

    async fn cached_client_projects(&self) -> Result<KleerClientProjectList, TimeTrackingError> {
        let client = self.client.clone();
        self.metadata_cache
            .client_projects
            .try_get_with(self.metadata_cache_key.clone(), async move {
                client.list_client_projects().await.map_err(map_kleer_error)
            })
            .await
            .map_err(cached_load_error)
    }

    async fn cached_active_client_projects(
        &self,
    ) -> Result<KleerClientProjectList, TimeTrackingError> {
        let client = self.client.clone();
        self.metadata_cache
            .active_client_projects
            .try_get_with(self.metadata_cache_key.clone(), async move {
                client
                    .list_active_client_projects()
                    .await
                    .map_err(map_kleer_error)
            })
            .await
            .map_err(cached_load_error)
    }

    async fn cached_activities(&self) -> Result<KleerActivityList, TimeTrackingError> {
        let client = self.client.clone();
        self.metadata_cache
            .activities
            .try_get_with(self.metadata_cache_key.clone(), async move {
                client.list_activities().await.map_err(map_kleer_error)
            })
            .await
            .map_err(cached_load_error)
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
            .map_err(|_| TimeTrackingError::InvalidInput(format!("invalid {label}: {raw}")))
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
        let (projects, activities) = tokio::try_join!(
            self.cached_active_client_projects(),
            self.cached_activities()
        )?;

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

    async fn get_owned_event(
        &self,
        event_id: i64,
    ) -> Result<KleerEventReadable, TimeTrackingError> {
        let event = self
            .client
            .get_event(event_id)
            .await
            .map_err(map_kleer_event_lookup_error)?;

        self.ensure_event_owned_by_target_user(&event)?;
        Ok(event)
    }

    fn build_event_writable(
        target: VerifiedKleerEventTarget,
        start_time: time::OffsetDateTime,
        end_time: time::OffsetDateTime,
        note: &str,
        user_id: i64,
        operation_id: &str,
    ) -> KleerEventWritable {
        let note = Self::event_comment(note);

        KleerEventWritable {
            foreign_id: operation_id.to_string(),
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
            internal_comment: None,
        }
    }

    fn event_comment(note: &str) -> &str {
        if note.trim().is_empty() {
            Self::MISSING_NOTE_COMMENT
        } else {
            note
        }
    }

    fn absence_event_foreign_id(user_id: i64, activity_id: i64, date: Date) -> String {
        format!(
            "toki-absence-{user_id}-{activity_id}-{date}-{}",
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        )
    }

    async fn load_absence_activity_map(&self) -> Result<AbsenceActivityMap, TimeTrackingError> {
        let activities = self.cached_activities().await?;

        Self::build_absence_activity_map(&activities.activity_readables)
    }

    fn build_absence_activity_map(
        activities: &[KleerActivityReadable],
    ) -> Result<AbsenceActivityMap, TimeTrackingError> {
        let mut ids_by_name: HashMap<&str, i64> = HashMap::new();
        let mut duplicate_names = HashSet::new();

        for activity in activities {
            let name = activity.name.trim();
            if KLEER_ABSENCE_ACTIVITY_NAMES
                .iter()
                .any(|(_, absence_name)| *absence_name == name)
                && ids_by_name.insert(name, activity.id.id).is_some()
            {
                duplicate_names.insert(name.to_string());
            }
        }

        if !duplicate_names.is_empty() {
            tracing::warn!(
                "duplicate Kleer absence activity names make writes ambiguous: {:?}",
                duplicate_names
            );
            return Err(TimeTrackingError::provider_unavailable(
                "Duplicate Kleer absence activity names make absence reporting ambiguous",
            ));
        }

        let mut by_type = HashMap::new();
        let mut by_activity_id = HashMap::new();
        for (absence_type, name) in KLEER_ABSENCE_ACTIVITY_NAMES {
            if let Some(activity_id) = ids_by_name.get(name).copied() {
                by_type.insert(*absence_type, activity_id);
                by_activity_id.insert(activity_id, *absence_type);
            }
        }

        Ok(AbsenceActivityMap {
            by_type,
            by_activity_id,
        })
    }

    fn absence_activity_id(
        absence_type: AbsenceType,
        map: &AbsenceActivityMap,
    ) -> Result<i64, TimeTrackingError> {
        map.by_type.get(&absence_type).copied().ok_or_else(|| {
            TimeTrackingError::InvalidInput("This absence type is not available in Kleer".into())
        })
    }

    fn absence_type_for_activity(
        activity_id: i64,
        map: &AbsenceActivityMap,
    ) -> Option<AbsenceType> {
        map.by_activity_id.get(&activity_id).copied()
    }

    fn build_absence_event_writable(
        request: &CreateAbsencesRequest,
        date: Date,
        hours: f64,
        activity_id: i64,
        user_id: i64,
    ) -> KleerEventWritable {
        KleerEventWritable {
            foreign_id: Self::absence_event_foreign_id(user_id, activity_id, date),
            user: KleerIdRef { id: user_id },
            activity: KleerIdRef { id: activity_id },
            client_project: None,
            child: request
                .child
                .as_ref()
                .map(|child| child.trim().to_string())
                .filter(|child| !child.is_empty()),
            date,
            hours,
            comment: request.comment.clone(),
            internal_comment: None,
        }
    }

    fn to_domain_absence_entry_from_event(
        event: &KleerEventReadable,
        map: &AbsenceActivityMap,
    ) -> Option<AbsenceEntry> {
        let absence_type = Self::absence_type_for_activity(event.activity.id, map)?;
        to_domain_absence_entry_from_event(event, absence_type)
    }

    fn to_owned_domain_absence_entry_from_event(
        &self,
        event: &KleerEventReadable,
        map: &AbsenceActivityMap,
    ) -> Option<AbsenceEntry> {
        self.ensure_event_owned_by_target_user(event).ok()?;
        Self::to_domain_absence_entry_from_event(event, map)
    }

    fn event_hours(events: &[KleerEventReadable], map: &AbsenceActivityMap) -> (f64, f64) {
        let worked_hours = events
            .iter()
            .filter(|event| event.client_project.is_some())
            .map(|event| event.hours)
            .sum();
        let absence_hours = events
            .iter()
            .filter(|event| {
                event.client_project.is_none()
                    && Self::absence_type_for_activity(event.activity.id, map).is_some()
            })
            .map(|event| event.hours)
            .sum();

        (worked_hours, absence_hours)
    }

    fn verify_deletable_absence_event(
        &self,
        event: &KleerEventReadable,
        map: &AbsenceActivityMap,
        date: Date,
    ) -> Result<(), TimeTrackingError> {
        self.ensure_event_owned_by_target_user(event)?;
        if event.date != date {
            return Err(TimeTrackingError::TimerNotFound);
        }
        if event.client_project.is_some() {
            return Err(TimeTrackingError::TimerNotFound);
        }
        if Self::absence_type_for_activity(event.activity.id, map).is_none() {
            return Err(TimeTrackingError::TimerNotFound);
        }
        Ok(())
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

    fn inclusive_dates(from: Date, to: Date) -> Vec<Date> {
        let mut dates = Vec::new();
        let mut date = from;
        while date <= to {
            dates.push(date);
            date += Duration::days(1);
        }
        dates
    }
}

#[async_trait]
impl TimeTrackingClient for KleerAdapter {
    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        let projects = self.cached_active_client_projects().await?;

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
        let (projects, activities) = tokio::try_join!(
            self.cached_active_client_projects(),
            self.cached_activities()
        )?;

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
            .or_else(default_for_missing_payroll_user)?;
        let absence_activity_map = self.load_absence_activity_map().await?;

        let scheduled_hours = to_domain_scheduled_hours(&schedule.payroll_user_schedule_metadatas);
        let (worked_hours, absence_hours) =
            Self::event_hours(&events.event_readables, &absence_activity_map);

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
        let (projects, activities, events, statuses) = tokio::try_join!(
            self.cached_client_projects(),
            self.cached_activities(),
            async {
                self.client
                    .list_events(self.target_user_id, date_range.0, date_range.1)
                    .await
                    .map_err(map_kleer_error)
            },
            async {
                self.client
                    .list_event_statuses(self.target_user_id, date_range.0, date_range.1)
                    .await
                    .map_err(map_kleer_error)
            },
        )?;

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
        operation_id: &str,
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
            operation_id,
        );
        let saved = self
            .client
            .create_event(&payload)
            .await
            .map_err(map_kleer_event_write_error)?;

        Ok(TimerId::new(saved.id.to_string()))
    }

    async fn find_time_entry_by_operation_id(
        &self,
        operation_id: &str,
        date: Date,
    ) -> Result<Option<TimerId>, TimeTrackingError> {
        let events = self
            .client
            .list_events(self.target_user_id, date, date)
            .await
            .map_err(map_kleer_error)?;
        let mut matches = events
            .event_readables
            .into_iter()
            .filter(|event| event.foreign_id == operation_id)
            .map(|event| TimerId::new(event.id.id.to_string()));
        let first = matches.next();
        if matches.next().is_some() {
            return Err(TimeTrackingError::provider_unavailable(format!(
                "multiple Kleer events use operation id {operation_id}"
            )));
        }
        Ok(first)
    }

    async fn get_time_entry(&self, registration_id: &str) -> Result<TimeEntry, TimeTrackingError> {
        let event_id = Self::parse_kleer_id(registration_id, "event id")?;
        let event = self.get_owned_event(event_id).await?;
        let project_id = event
            .client_project
            .as_ref()
            .ok_or(TimeTrackingError::TimerNotFound)?
            .id;
        let (projects, activities, statuses) = tokio::try_join!(
            self.cached_client_projects(),
            self.cached_activities(),
            async {
                self.client
                    .list_event_statuses(self.target_user_id, event.date, event.date)
                    .await
                    .map_err(map_kleer_error)
            },
        )?;
        let project_name = projects
            .client_project_readables
            .iter()
            .find(|project| project.id.id == project_id)
            .map(|project| project.name.clone())
            .ok_or_else(|| TimeTrackingError::ProjectNotFound(project_id.to_string()))?;
        let activity_name = activities
            .activity_readables
            .iter()
            .find(|activity| activity.id.id == event.activity.id)
            .map(|activity| activity.name.clone())
            .ok_or_else(|| TimeTrackingError::ActivityNotFound(event.activity.id.to_string()))?;
        let status = event
            .status
            .as_ref()
            .map(|status| to_domain_status(status.status_type.clone()))
            .or_else(|| {
                Self::to_domain_day_statuses(statuses)
                    .into_iter()
                    .find(|status| status.date == event.date)
                    .map(|status| status.status)
            })
            .unwrap_or_default();

        to_domain_time_entry(&event, project_name, activity_name, status)
    }

    async fn edit_time_entry(
        &self,
        registration_id: &str,
        request: &EditTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        let event_id = Self::parse_kleer_id(registration_id, "event id")?;
        let existing = self.get_owned_event(event_id).await?;
        let target = self
            .ensure_project_activity_allowed(&request.project_id, &request.activity_id)
            .await?;

        let fallback_operation_id = format!("toki-entry-{}-{event_id}", self.target_user_id);
        let payload = Self::build_event_writable(
            target,
            request.start_time,
            request.end_time,
            &request.note,
            self.target_user_id,
            if existing.foreign_id.is_empty() {
                &fallback_operation_id
            } else {
                &existing.foreign_id
            },
        );
        let saved = self
            .client
            .update_event(event_id, &payload)
            .await
            .map_err(map_kleer_event_write_error)?;

        Ok(TimerId::new(saved.id.to_string()))
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        let event_id = Self::parse_kleer_id(registration_id, "event id")?;
        self.get_owned_event(event_id).await?;

        self.client
            .delete_event(event_id)
            .await
            .map_err(map_kleer_event_write_error)?;
        Ok(())
    }

    async fn get_absence_types(&self) -> Result<Vec<AbsenceType>, TimeTrackingError> {
        let map = self.load_absence_activity_map().await?;
        Ok(KLEER_ABSENCE_ACTIVITY_NAMES
            .iter()
            .filter_map(|(absence_type, _)| {
                map.by_type
                    .contains_key(absence_type)
                    .then_some(*absence_type)
            })
            .collect())
    }

    async fn get_absence_children(&self) -> Result<Vec<AbsenceChild>, TimeTrackingError> {
        let payroll_user = self
            .client
            .get_payroll_user(self.target_user_id)
            .await
            .or_else(default_for_missing_payroll_user)?;

        let mut children: Vec<_> = payroll_user
            .children
            .into_iter()
            .map(to_domain_absence_child)
            .collect();
        children.sort_by(|a, b| a.name.cmp(&b.name).then(a.birth_date.cmp(&b.birth_date)));
        Ok(children)
    }

    async fn get_absences(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        let absence_activity_map = self.load_absence_activity_map().await?;
        let events = self
            .client
            .list_events(self.target_user_id, date_range.0, date_range.1)
            .await
            .map_err(map_kleer_error)?;

        let mut entries: Vec<_> = events
            .event_readables
            .iter()
            .filter_map(|event| {
                self.to_owned_domain_absence_entry_from_event(event, &absence_activity_map)
            })
            .collect();
        entries.sort_by(|a, b| b.date.cmp(&a.date).then(a.absence_id.cmp(&b.absence_id)));
        Ok(entries)
    }

    async fn get_absence_day_defaults(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceDayDefault>, TimeTrackingError> {
        let schedule = self
            .client
            .list_schedule_summary(self.target_user_id, date_range.0, date_range.1)
            .await
            .or_else(default_for_missing_payroll_user)?;
        let hours_by_date: HashMap<_, _> = schedule
            .payroll_user_schedule_metadatas
            .into_iter()
            .map(|day| (day.date, day.actual_hours))
            .collect();

        Ok(Self::inclusive_dates(date_range.0, date_range.1)
            .into_iter()
            .map(|date| AbsenceDayDefault {
                date,
                scheduled_hours: hours_by_date.get(&date).copied().unwrap_or(0.0),
            })
            .collect())
    }

    async fn create_absences(
        &self,
        request: &CreateAbsencesRequest,
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        let absence_activity_map = self.load_absence_activity_map().await?;
        let activity_id = Self::absence_activity_id(request.absence_type, &absence_activity_map)?;
        let mut created = Vec::new();

        for day in &request.days {
            let payload = Self::build_absence_event_writable(
                request,
                day.date,
                day.hours,
                activity_id,
                self.target_user_id,
            );
            match self.client.create_event(&payload).await {
                Ok(saved) => {
                    created.push(AbsenceEntry {
                        absence_id: saved.id.to_string(),
                        date: day.date,
                        hours: day.hours,
                        absence_type: request.absence_type,
                        child: payload.child,
                        comment: Some(request.comment.clone())
                            .filter(|value| !value.trim().is_empty()),
                        managed: true,
                        deletable: true,
                    });
                }
                Err(error) => {
                    for entry in &created {
                        if let Ok(event_id) = Self::parse_kleer_id(&entry.absence_id, "event id") {
                            if let Err(rollback_error) = self.client.delete_event(event_id).await {
                                tracing::warn!(
                                    "Failed to rollback Kleer absence event {}: {:?}",
                                    entry.absence_id,
                                    rollback_error
                                );
                            }
                        }
                    }
                    return Err(map_kleer_event_write_error(error));
                }
            }
        }

        Ok(created)
    }

    async fn delete_absence(&self, absence_id: &str, date: Date) -> Result<(), TimeTrackingError> {
        let event_id = Self::parse_kleer_id(absence_id, "event id")?;
        let event = self
            .client
            .get_event(event_id)
            .await
            .map_err(map_kleer_event_lookup_error)?;
        let absence_activity_map = self.load_absence_activity_map().await?;
        self.verify_deletable_absence_event(&event, &absence_activity_map, date)?;

        self.client
            .delete_event(event_id)
            .await
            .map_err(map_kleer_event_write_error)?;
        Ok(())
    }
}

fn map_kleer_error(error: KleerError) -> TimeTrackingError {
    match error {
        KleerError::NotFound => TimeTrackingError::TimerNotFound,
        KleerError::Unauthorized => {
            TimeTrackingError::provider_unavailable("Kleer integration token is invalid or expired")
        }
        KleerError::Forbidden => {
            TimeTrackingError::Forbidden("time tracking provider denied access".into())
        }
        KleerError::InvalidConfig(message) => TimeTrackingError::provider_unavailable(message),
        KleerError::Request(message) => TimeTrackingError::provider_unavailable(message),
        KleerError::Deserialize { message, .. } => TimeTrackingError::provider_unavailable(message),
        KleerError::Response { status, body } => {
            let message = kleer_response_message(&body);
            if is_single_access_denied_message(&message) {
                return TimeTrackingError::Forbidden("time tracking provider denied access".into());
            }
            tracing::warn!("Kleer returned non-success response: status={status}, body={message}");
            TimeTrackingError::provider_unavailable(format!("Kleer returned {status}: {message}"))
        }
    }
}

fn map_kleer_event_write_error(error: KleerError) -> TimeTrackingError {
    if is_event_access_denied(&error) {
        return TimeTrackingError::Forbidden("time tracking provider denied access".into());
    }
    if is_locked_period(&error) {
        return TimeTrackingError::LockedPeriod;
    }

    map_kleer_error(error)
}

fn map_kleer_event_lookup_error(error: KleerError) -> TimeTrackingError {
    if is_event_access_denied(&error) {
        return TimeTrackingError::TimerNotFound;
    }

    map_kleer_event_write_error(error)
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

fn default_for_missing_payroll_user<T: Default>(error: KleerError) -> Result<T, TimeTrackingError> {
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

fn is_locked_period(error: &KleerError) -> bool {
    match error {
        KleerError::Response { body, .. } => {
            let body = body.to_lowercase();
            body.contains("locked")
                || body.contains("paid")
                || body.contains("approved")
                || body.contains("certified")
                || body.contains("utbetald")
                || body.contains("låst")
        }
        _ => false,
    }
}

fn is_event_access_denied(error: &KleerError) -> bool {
    matches!(
        error,
        KleerError::Response { body, .. } if body.contains("EventAccessDeniedException")
    )
}

fn is_single_access_denied_message(message: &str) -> bool {
    message.contains("SingleAccessDeniedException")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kleer::{
        KleerEventStatus, KleerProjectActivityAssignment, KleerProjectUserAssignment,
        KleerStatusType,
    };
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

    fn activity(id: i64, name: &str) -> KleerActivityReadable {
        KleerActivityReadable {
            id: KleerIdRef { id },
            name: name.to_string(),
            description: String::new(),
            mandatory_child_when_reporting: false,
        }
    }

    fn all_absence_activities() -> Vec<KleerActivityReadable> {
        KLEER_ABSENCE_ACTIVITY_NAMES
            .iter()
            .enumerate()
            .map(|(index, (_, name))| activity(10_000 + index as i64, name))
            .collect()
    }

    fn event(
        id: i64,
        user_id: i64,
        activity_id: i64,
        client_project_id: Option<i64>,
        hours: f64,
    ) -> KleerEventReadable {
        KleerEventReadable {
            id: KleerIdRef { id },
            foreign_id: String::new(),
            user: KleerIdRef { id: user_id },
            activity: KleerIdRef { id: activity_id },
            client_project: client_project_id.map(|id| KleerIdRef { id }),
            child: None,
            date: event_date(),
            hours,
            comment: Some("comment".to_string()),
            internal_comment: None,
            approved: None,
            status: Some(KleerEventStatus {
                status_type: KleerStatusType::Open,
                registration_user: None,
                registration_date: None,
                event_date: None,
            }),
        }
    }

    fn event_date() -> Date {
        Date::from_calendar_date(2026, Month::May, 20).unwrap()
    }

    fn adapter() -> KleerAdapter {
        KleerAdapter::with_metadata_cache(
            KleerCredentials::new("token", "company", None::<String>),
            TARGET_USER_ID,
            Arc::new(KleerMetadataCache::new()),
        )
        .unwrap()
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
    fn event_payload_saves_note_as_external_comment_only() {
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
            "toki-op-test",
        );

        assert_eq!(payload.comment, "Worked on PR review");
        assert_eq!(payload.internal_comment, None);
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
                "toki-op-test",
            );

            assert_eq!(payload.comment, KleerAdapter::MISSING_NOTE_COMMENT);
            assert_eq!(payload.internal_comment, None);
        }
    }

    #[test]
    fn absence_activity_map_resolves_all_known_swedish_names() {
        let activities = all_absence_activities();
        let map = KleerAdapter::build_absence_activity_map(&activities).unwrap();

        assert_eq!(map.by_type.len(), KLEER_ABSENCE_ACTIVITY_NAMES.len());
        assert_eq!(map.by_type.get(&AbsenceType::Sick).copied(), Some(10_004));
        assert_eq!(
            map.by_activity_id.get(&10_004).copied(),
            Some(AbsenceType::Sick)
        );
    }

    #[test]
    fn missing_absence_activity_names_are_omitted() {
        let activities = vec![activity(96858, "Sjuk")];
        let map = KleerAdapter::build_absence_activity_map(&activities).unwrap();

        assert_eq!(map.by_type.len(), 1);
        assert_eq!(map.by_type.get(&AbsenceType::Sick).copied(), Some(96858));
    }

    #[test]
    fn duplicate_absence_activity_names_are_an_error() {
        let activities = vec![activity(1, "Sjuk"), activity(2, " Sjuk ")];
        let error = KleerAdapter::build_absence_activity_map(&activities).unwrap_err();

        assert!(matches!(error, TimeTrackingError::ProviderUnavailable(_)));
    }

    #[test]
    fn absence_event_payload_uses_projectless_event_fields() {
        let date = Date::from_calendar_date(2026, Month::May, 20).unwrap();
        let payload = KleerAdapter::build_absence_event_writable(
            &CreateAbsencesRequest {
                absence_type: AbsenceType::Sick,
                child: None,
                comment: "Flu".to_string(),
                days: vec![],
            },
            date,
            8.0,
            96858,
            TARGET_USER_ID,
        );
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(payload.user.id, TARGET_USER_ID);
        assert_eq!(payload.activity.id, 96858);
        assert_eq!(payload.client_project, None);
        assert_eq!(payload.date, date);
        assert_eq!(payload.hours, 8.0);
        assert_eq!(payload.comment, "Flu");
        assert!(payload
            .foreign_id
            .starts_with(&format!("toki-absence-{TARGET_USER_ID}-96858-2026-05-20-")));
        assert!(json.get("client-project").is_none());
    }

    #[test]
    fn projectless_resolved_event_converts_to_managed_absence() {
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();
        let entry = KleerAdapter::to_domain_absence_entry_from_event(
            &event(1, TARGET_USER_ID, 96858, None, 4.0),
            &map,
        )
        .unwrap();

        assert_eq!(entry.absence_type, AbsenceType::Sick);
        assert!(entry.managed);
        assert!(entry.deletable);
    }

    #[test]
    fn owned_absence_conversion_rejects_other_users_events() {
        let adapter = adapter();
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();

        assert!(adapter
            .to_owned_domain_absence_entry_from_event(
                &event(1, TARGET_USER_ID + 1, 96858, None, 4.0),
                &map,
            )
            .is_none());
    }

    #[test]
    fn time_entry_ownership_does_not_reveal_other_users_entries() {
        let adapter = adapter();
        let other_users_entry = event(1, TARGET_USER_ID + 1, 123, Some(456), 4.0);

        let error = adapter
            .ensure_event_owned_by_target_user(&other_users_entry)
            .unwrap_err();

        assert!(matches!(error, TimeTrackingError::TimerNotFound));
    }

    #[test]
    fn project_backed_or_unknown_projectless_events_are_not_absences() {
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();

        assert!(KleerAdapter::to_domain_absence_entry_from_event(
            &event(1, TARGET_USER_ID, 96858, Some(123), 8.0),
            &map,
        )
        .is_none());
        assert!(KleerAdapter::to_domain_absence_entry_from_event(
            &event(2, TARGET_USER_ID, 111, None, 8.0),
            &map,
        )
        .is_none());
    }

    #[test]
    fn time_info_counts_project_events_and_known_absence_events() {
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();
        let events = vec![
            event(1, TARGET_USER_ID, 10, Some(123), 6.0),
            event(2, TARGET_USER_ID, 96858, None, 8.0),
            event(3, TARGET_USER_ID, 999, None, 2.0),
        ];

        assert_eq!(KleerAdapter::event_hours(&events, &map), (6.0, 8.0));
    }

    #[test]
    fn delete_validation_rejects_non_absence_events() {
        let adapter = adapter();
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();

        assert!(matches!(
            adapter.verify_deletable_absence_event(
                &event(1, TARGET_USER_ID + 1, 96858, None, 8.0),
                &map,
                event_date(),
            ),
            Err(TimeTrackingError::TimerNotFound)
        ));
        assert!(matches!(
            adapter.verify_deletable_absence_event(
                &event(2, TARGET_USER_ID, 96858, Some(123), 8.0),
                &map,
                event_date(),
            ),
            Err(TimeTrackingError::TimerNotFound)
        ));
        assert!(matches!(
            adapter.verify_deletable_absence_event(
                &event(3, TARGET_USER_ID, 111, None, 8.0),
                &map,
                event_date(),
            ),
            Err(TimeTrackingError::TimerNotFound)
        ));
    }

    #[test]
    fn delete_validation_rejects_wrong_absence_date() {
        let adapter = adapter();
        let map = KleerAdapter::build_absence_activity_map(&[activity(96858, "Sjuk")]).unwrap();

        assert!(matches!(
            adapter.verify_deletable_absence_event(
                &event(1, TARGET_USER_ID, 96858, None, 8.0),
                &map,
                event_date() + Duration::days(1),
            ),
            Err(TimeTrackingError::TimerNotFound)
        ));
    }

    #[test]
    fn absence_event_access_denied_maps_to_forbidden() {
        let error = map_kleer_event_write_error(KleerError::Response {
            status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            body: r#"{"message":"EventAccessDeniedException: denied"}"#.to_string(),
        });

        assert!(matches!(error, TimeTrackingError::Forbidden(_)));
    }

    #[test]
    fn single_access_denied_maps_to_forbidden() {
        let error = map_kleer_error(KleerError::Response {
            status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            body: r#"{"message":"SingleAccessDeniedException: Otillåten access"}"#.to_string(),
        });

        assert!(matches!(error, TimeTrackingError::Forbidden(_)));
    }

    #[test]
    fn provider_transport_configuration_and_server_failures_are_unavailable() {
        let errors = [
            KleerError::InvalidConfig("missing token".into()),
            KleerError::Request("connection refused".into()),
            KleerError::Deserialize {
                message: "invalid response".into(),
                body: "secret response".into(),
            },
            KleerError::Response {
                status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                body: r#"{"message":"provider exploded"}"#.into(),
            },
            KleerError::Unauthorized,
        ];

        for error in errors {
            assert!(matches!(
                map_kleer_error(error),
                TimeTrackingError::ProviderUnavailable(_)
            ));
        }
    }

    #[test]
    fn provider_forbidden_and_not_found_keep_domain_status_semantics() {
        assert!(matches!(
            map_kleer_error(KleerError::Forbidden),
            TimeTrackingError::Forbidden(_)
        ));
        assert!(matches!(
            map_kleer_error(KleerError::NotFound),
            TimeTrackingError::TimerNotFound
        ));
    }
}
