mod time_entry_writes;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use itertools::Itertools;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, ActiveTimer, Activity,
        CreateAbsencesRequest, CreateTimeEntryRequest, EditTimeEntryRequest, PatchValue, Project,
        ProjectId, StartTimerRequest, TimeEntry, TimeEntryDayStatus, TimeEntryStatus,
        TimerHistoryEntry, UpdateTimerRequest, UserId, WeeklyStats,
    },
    ports::{
        inbound::TimeTrackingService,
        outbound::{TimeEntryWriteStore, TimeTrackingClient, TimerHistoryRepository},
    },
    TimeTrackingError,
};

/// Implementation of the TimeTrackingService inbound port.
///
/// This service orchestrates time tracking operations by delegating to a
/// TimeTrackingClient (outbound port) and adding business logic.
///
/// Always requires local timer history and durable write storage.
pub struct TimeTrackingServiceImpl<C, R> {
    client: Arc<C>,
    timer_repo: Arc<R>,
}

impl<C: TimeTrackingClient, R: TimerHistoryRepository + TimeEntryWriteStore>
    TimeTrackingServiceImpl<C, R>
{
    pub fn new(client: Arc<C>, timer_repo: Arc<R>) -> Self {
        Self { client, timer_repo }
    }

    fn time_entry_from_edit_request(
        request: &EditTimeEntryRequest,
        project: &Project,
        activity: &Activity,
        registration_id: impl Into<String>,
    ) -> TimeEntry {
        let date = request.start_time.date();
        let hours = (request.end_time - request.start_time).whole_seconds() as f64 / 3600.0;

        TimeEntry::new(
            registration_id,
            request.project_id.clone(),
            project.name.clone(),
            request.activity_id.clone(),
            activity.name.clone(),
            date,
            hours,
        )
        .with_note(request.note.clone())
        .with_times(Some(request.start_time), Some(request.end_time))
        .with_week_number(date.iso_week())
        .with_status(TimeEntryStatus::Open)
    }

    fn validate_interval(
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
    ) -> Result<(), TimeTrackingError> {
        if end_time <= start_time {
            return Err(TimeTrackingError::InvalidInput(
                "end time must be after start time".to_string(),
            ));
        }
        if end_time - start_time > time::Duration::hours(24) {
            return Err(TimeTrackingError::InvalidInput(
                "time entry duration cannot exceed 24 hours".to_string(),
            ));
        }
        Ok(())
    }

    async fn resolve_required_selection(
        &self,
        project_id: &ProjectId,
        activity_id: &crate::domain::models::ActivityId,
        date: Date,
    ) -> Result<(Project, Activity), TimeTrackingError> {
        let project = self
            .client
            .get_projects()
            .await?
            .into_iter()
            .find(|project| project.id == *project_id)
            .ok_or_else(|| {
                TimeTrackingError::InvalidProjectActivity(format!(
                    "project {} is not available",
                    project_id
                ))
            })?;
        let activity = self
            .client
            .get_activities(project_id, (date, date))
            .await?
            .into_iter()
            .find(|activity| activity.id == *activity_id)
            .ok_or_else(|| {
                TimeTrackingError::InvalidProjectActivity(format!(
                    "activity {} is not available for project {}",
                    activity_id, project_id
                ))
            })?;
        Ok((project, activity))
    }

    async fn resolve_optional_selection(
        &self,
        project_id: Option<&ProjectId>,
        activity_id: Option<&crate::domain::models::ActivityId>,
        date: Date,
    ) -> Result<(Option<Project>, Option<Activity>), TimeTrackingError> {
        match (project_id, activity_id) {
            (None, Some(_)) => Err(TimeTrackingError::InvalidProjectActivity(
                "an activity cannot be selected without a project".to_string(),
            )),
            (None, None) => Ok((None, None)),
            (Some(project_id), Some(activity_id)) => {
                let (project, activity) = self
                    .resolve_required_selection(project_id, activity_id, date)
                    .await?;
                Ok((Some(project), Some(activity)))
            }
            (Some(project_id), None) => {
                let project = self
                    .client
                    .get_projects()
                    .await?
                    .into_iter()
                    .find(|project| project.id == *project_id)
                    .ok_or_else(|| {
                        TimeTrackingError::InvalidProjectActivity(format!(
                            "project {} is not available",
                            project_id
                        ))
                    })?;
                Ok((Some(project), None))
            }
        }
    }

    async fn ensure_day_is_open(&self, date: Date) -> Result<(), TimeTrackingError> {
        let locked = self
            .client
            .get_time_entry_day_statuses((date, date))
            .await?
            .into_iter()
            .any(|status| status.date == date && status.status != TimeEntryStatus::Open);
        if locked {
            Err(TimeTrackingError::LockedPeriod)
        } else {
            Ok(())
        }
    }

    fn trimmed_child_name(request: &CreateAbsencesRequest) -> Option<&str> {
        request
            .child
            .as_deref()
            .map(str::trim)
            .filter(|child| !child.is_empty())
    }

    fn validate_create_absences(request: &CreateAbsencesRequest) -> Result<(), TimeTrackingError> {
        if request.days.is_empty() {
            return Err(TimeTrackingError::InvalidInput(
                "At least one absence day is required".to_string(),
            ));
        }

        if request.absence_type.requires_child() && Self::trimmed_child_name(request).is_none() {
            return Err(TimeTrackingError::InvalidInput(
                "Child name is required for this absence type".to_string(),
            ));
        }

        let mut dates = HashSet::new();
        for day in &request.days {
            if !dates.insert(day.date) {
                return Err(TimeTrackingError::InvalidInput(format!(
                    "Duplicate absence date: {}",
                    day.date
                )));
            }
            if !day.hours.is_finite() || day.hours <= 0.0 {
                return Err(TimeTrackingError::InvalidInput(
                    "Absence hours must be greater than 0".to_string(),
                ));
            }
            if day.hours > 24.0 {
                return Err(TimeTrackingError::InvalidInput(
                    "Absence hours cannot exceed 24 per day".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn validate_registered_child(
        requested_child: &str,
        children: &[AbsenceChild],
    ) -> Result<(), TimeTrackingError> {
        if children.is_empty() {
            return Err(TimeTrackingError::InvalidInput(
                "Register a child in Kleer before reporting this absence type".to_string(),
            ));
        }

        if !children
            .iter()
            .any(|registered| registered.name == requested_child)
        {
            return Err(TimeTrackingError::InvalidInput(
                "Select a registered child for this absence type".to_string(),
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl<C: TimeTrackingClient, R: TimerHistoryRepository + TimeEntryWriteStore> TimeTrackingService
    for TimeTrackingServiceImpl<C, R>
{
    // ========================================================================
    // Active Timer Operations (local DB via TimerHistoryRepository)
    // ========================================================================

    async fn get_active_timer(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        self.timer_repo.get_active_timer(user_id).await
    }

    async fn start_timer(
        &self,
        user_id: &UserId,
        request: &StartTimerRequest,
    ) -> Result<ActiveTimer, TimeTrackingError> {
        if self.timer_repo.get_active_timer(user_id).await?.is_some() {
            return Err(TimeTrackingError::TimerAlreadyRunning);
        }

        let started_at = OffsetDateTime::now_utc();
        let (project, activity) = self
            .resolve_optional_selection(
                request.project_id.as_ref(),
                request.activity_id.as_ref(),
                started_at.date(),
            )
            .await?;
        let mut timer = ActiveTimer::new(started_at).with_note(request.note.clone());
        if let Some(project) = project {
            timer = timer.with_project(project.id, project.name);
        }
        if let Some(activity) = activity {
            timer = timer.with_activity(activity.id, activity.name);
        }

        self.timer_repo.create_timer(user_id, &timer).await?;
        Ok(timer)
    }

    async fn stop_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError> {
        self.timer_repo.delete_timer(user_id).await
    }

    async fn save_timer(
        &self,
        user_id: &UserId,
        note: Option<String>,
        idempotency_key: &str,
    ) -> Result<TimeEntry, TimeTrackingError> {
        self.save_active_time_entry(user_id, note, idempotency_key)
            .await
    }

    async fn edit_timer(
        &self,
        user_id: &UserId,
        request: &UpdateTimerRequest,
    ) -> Result<ActiveTimer, TimeTrackingError> {
        let current = self
            .timer_repo
            .get_active_timer(user_id)
            .await?
            .ok_or(TimeTrackingError::NoTimerRunning)?;
        let started_at = request.started_at.unwrap_or(current.started_at);
        if started_at > OffsetDateTime::now_utc() {
            return Err(TimeTrackingError::InvalidInput(
                "timer start time cannot be in the future".to_string(),
            ));
        }

        let project_id = match &request.project_id {
            PatchValue::Unchanged => current.project_id.clone(),
            PatchValue::Clear => None,
            PatchValue::Set(project_id) => Some(project_id.clone()),
        };
        let project_changed = project_id != current.project_id;
        let activity_id = if project_id.is_none() {
            match request.activity_id {
                PatchValue::Set(_) => {
                    return Err(TimeTrackingError::InvalidProjectActivity(
                        "an activity cannot be selected without a project".to_string(),
                    ))
                }
                PatchValue::Unchanged | PatchValue::Clear => None,
            }
        } else {
            match &request.activity_id {
                PatchValue::Unchanged if project_changed => None,
                PatchValue::Unchanged => current.activity_id.clone(),
                PatchValue::Clear => None,
                PatchValue::Set(activity_id) => Some(activity_id.clone()),
            }
        };
        let (project, activity) = self
            .resolve_optional_selection(
                project_id.as_ref(),
                activity_id.as_ref(),
                started_at.date(),
            )
            .await?;
        let mut timer =
            ActiveTimer::new(started_at).with_note(request.note.clone().unwrap_or(current.note));
        if let Some(project) = project {
            timer = timer.with_project(project.id, project.name);
        }
        if let Some(activity) = activity {
            timer = timer.with_activity(activity.id, activity.name);
        }

        self.timer_repo.update_timer(user_id, &timer).await?;
        Ok(timer)
    }

    // ========================================================================
    // Project/Activity Lookups
    // ========================================================================

    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        self.client.get_projects().await
    }

    async fn get_activities(
        &self,
        project_id: &ProjectId,
        date_range: (Date, Date),
    ) -> Result<Vec<Activity>, TimeTrackingError> {
        self.client.get_activities(project_id, date_range).await
    }

    // ========================================================================
    // Calendar/Time Entry Operations
    // ========================================================================

    async fn get_time_info(
        &self,
        date_range: (Date, Date),
    ) -> Result<WeeklyStats, TimeTrackingError> {
        self.client.get_time_info(date_range).await
    }

    async fn get_time_entries(
        &self,
        user_id: &UserId,
        date_range: (Date, Date),
        unique: bool,
    ) -> Result<Vec<TimeEntry>, TimeTrackingError> {
        // Get entries from the provider
        let mut entries = self.client.get_time_entries(date_range).await?;

        // Merge with local timer history
        let history = self.timer_repo.get_history(user_id).await?;

        // Build a map of registration_id -> (start_time, end_time)
        let history_map: HashMap<String, _> = history
            .into_iter()
            .filter_map(|h| {
                h.registration_id
                    .map(|reg_id| (reg_id, (h.start_time, h.end_time)))
            })
            .collect();

        // Augment entries with local start/end times
        entries = entries
            .into_iter()
            .map(|entry| {
                if let Some((start_time, end_time)) = history_map.get(&entry.registration_id) {
                    entry.with_times(Some(*start_time), *end_time)
                } else {
                    entry
                }
            })
            .collect();

        // Sort by date (descending) then by start_time (descending)
        entries.sort_by(|a, b| {
            let date_cmp = b.date.cmp(&a.date);
            if date_cmp == std::cmp::Ordering::Equal {
                b.start_time.cmp(&a.start_time)
            } else {
                date_cmp
            }
        });

        // Apply unique filter if requested
        if unique {
            entries = entries
                .into_iter()
                .unique_by(|entry| {
                    format!(
                        "{}-{}-{}",
                        entry.project_name,
                        entry.activity_name,
                        entry.note.as_ref().unwrap_or(&String::new())
                    )
                })
                .collect();
        }

        Ok(entries)
    }

    async fn get_time_entry_day_statuses(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntryDayStatus>, TimeTrackingError> {
        self.client.get_time_entry_day_statuses(date_range).await
    }

    async fn create_time_entry(
        &self,
        user_id: &UserId,
        request: &CreateTimeEntryRequest,
        idempotency_key: &str,
    ) -> Result<TimeEntry, TimeTrackingError> {
        self.create_prepared_time_entry(user_id, request, idempotency_key)
            .await
    }

    async fn edit_time_entry(
        &self,
        registration_id: &str,
        request: &EditTimeEntryRequest,
    ) -> Result<TimeEntry, TimeTrackingError> {
        Self::validate_interval(request.start_time, request.end_time)?;
        let current = self.client.get_time_entry(registration_id).await?;
        if current.status != TimeEntryStatus::Open {
            return Err(TimeTrackingError::LockedPeriod);
        }
        self.ensure_day_is_open(request.start_time.date()).await?;
        let (project, activity) = self
            .resolve_required_selection(
                &request.project_id,
                &request.activity_id,
                request.start_time.date(),
            )
            .await?;

        let new_registration_id = self
            .client
            .edit_time_entry(registration_id, request)
            .await?;
        let updated_entry = Self::time_entry_from_edit_request(
            request,
            &project,
            &activity,
            new_registration_id.to_string(),
        );

        // Update local timer history
        // Check if we have a local record for this registration
        if self
            .timer_repo
            .get_by_registration_id(registration_id)
            .await?
            .is_some()
        {
            // Check if registration ID changed (day changed)
            if new_registration_id.as_str() != registration_id {
                // Update both registration ID and times
                if let Err(e) = self
                    .timer_repo
                    .update_registration_and_times(
                        registration_id,
                        new_registration_id.as_str(),
                        &request.start_time,
                        &request.end_time,
                    )
                    .await
                {
                    tracing::error!("Failed to update timer history: {:?}", e);
                }
            } else {
                // Just update times
                if let Err(e) = self
                    .timer_repo
                    .update_times(registration_id, &request.start_time, &request.end_time)
                    .await
                {
                    tracing::error!("Failed to update timer times: {:?}", e);
                }
            }
        }

        Ok(updated_entry)
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        let existing = self.client.get_time_entry(registration_id).await?;
        if existing.status != TimeEntryStatus::Open {
            return Err(TimeTrackingError::LockedPeriod);
        }
        self.client.delete_time_entry(registration_id).await
        // Note: We don't delete from local timer history - it serves as an audit log
    }

    async fn get_absences(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        self.client.get_absences(date_range).await
    }

    async fn get_absence_types(&self) -> Result<Vec<AbsenceType>, TimeTrackingError> {
        self.client.get_absence_types().await
    }

    async fn get_absence_children(&self) -> Result<Vec<AbsenceChild>, TimeTrackingError> {
        self.client.get_absence_children().await
    }

    async fn get_absence_day_defaults(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<AbsenceDayDefault>, TimeTrackingError> {
        self.client.get_absence_day_defaults(date_range).await
    }

    async fn create_absences(
        &self,
        request: &CreateAbsencesRequest,
    ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
        Self::validate_create_absences(request)?;
        let available_types = self.client.get_absence_types().await?;
        if !available_types.contains(&request.absence_type) {
            return Err(TimeTrackingError::InvalidInput(
                "This absence type is not available in Kleer".to_string(),
            ));
        }
        if request.absence_type.requires_child() {
            let child = Self::trimmed_child_name(request)
                .expect("create_absences validates child-related absences before child lookup");
            let children = self.client.get_absence_children().await?;
            Self::validate_registered_child(child, &children)?;
        }
        self.client.create_absences(request).await
    }

    async fn delete_absence(&self, absence_id: &str, date: Date) -> Result<(), TimeTrackingError> {
        self.client.delete_absence(absence_id, date).await
    }

    async fn get_timer_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        self.timer_repo.get_history(user_id).await
    }
}

#[cfg(test)]
mod tests;
