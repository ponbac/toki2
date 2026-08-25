use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use itertools::Itertools;
use sha2::{Digest, Sha256};
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, ActiveTimer, Activity,
        CreateAbsencesRequest, CreateTimeEntryRequest, EditTimeEntryRequest, IdempotencyClaim,
        NewTimerHistoryEntry, PatchValue, Project, ProjectId, StartTimerRequest, TimeEntry,
        TimeEntryDayStatus, TimeEntryStatus, TimeTrackingWriteOperation, TimerHistoryEntry,
        UpdateTimerRequest, UserId, WeeklyStats,
    },
    ports::{
        inbound::TimeTrackingService,
        outbound::{TimeTrackingClient, TimerHistoryRepository},
    },
    TimeTrackingError,
};

/// Implementation of the TimeTrackingService inbound port.
///
/// This service orchestrates time tracking operations by delegating to a
/// TimeTrackingClient (outbound port) and adding business logic.
///
/// Always requires a TimerHistoryRepository — the factory ensures one is provided.
pub struct TimeTrackingServiceImpl<C, R> {
    client: Arc<C>,
    timer_repo: Arc<R>,
}

enum ClaimedWrite {
    Acquired { operation_id: String, resumed: bool },
    Replay(TimeEntry),
}

impl<C: TimeTrackingClient, R: TimerHistoryRepository> TimeTrackingServiceImpl<C, R> {
    pub fn new(client: Arc<C>, timer_repo: Arc<R>) -> Self {
        Self { client, timer_repo }
    }

    fn time_entry_from_create_request(
        request: &CreateTimeEntryRequest,
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

    fn validate_idempotency_key(key: &str) -> Result<&str, TimeTrackingError> {
        let key = key.trim();
        if key.is_empty() || key.len() > 200 {
            return Err(TimeTrackingError::InvalidInput(
                "Idempotency-Key must contain between 1 and 200 characters".to_string(),
            ));
        }
        Ok(key)
    }

    fn hash_bytes(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn provider_operation_id(
        user_id: &UserId,
        operation: TimeTrackingWriteOperation,
        key: &str,
    ) -> String {
        let source = format!("{}:{}:{key}", user_id.as_i32(), operation.as_str());
        format!("toki-op-{}", Self::hash_bytes(source.as_bytes()))
    }

    async fn claim_write(
        &self,
        user_id: &UserId,
        operation: TimeTrackingWriteOperation,
        key: &str,
        request_hash: &str,
    ) -> Result<ClaimedWrite, TimeTrackingError> {
        let claim = self
            .timer_repo
            .claim_idempotency(
                user_id,
                operation,
                key,
                request_hash,
                &Self::provider_operation_id(user_id, operation, key),
            )
            .await?;
        match claim {
            IdempotencyClaim::Fresh { operation_id } => Ok(ClaimedWrite::Acquired {
                operation_id,
                resumed: false,
            }),
            IdempotencyClaim::Resumed { operation_id } => Ok(ClaimedWrite::Acquired {
                operation_id,
                resumed: true,
            }),
            IdempotencyClaim::Replay(entry) => Ok(ClaimedWrite::Replay(entry)),
            IdempotencyClaim::InProgress => Err(TimeTrackingError::IdempotencyInProgress),
            IdempotencyClaim::PayloadMismatch => Err(TimeTrackingError::IdempotencyConflict),
        }
    }

    async fn release_write(
        &self,
        user_id: &UserId,
        operation: TimeTrackingWriteOperation,
        key: &str,
    ) {
        if let Err(error) = self
            .timer_repo
            .release_idempotency(user_id, operation, key)
            .await
        {
            tracing::error!(%error, "failed to release time-tracking idempotency claim");
        }
    }

    async fn create_or_reconcile_entry(
        &self,
        request: &CreateTimeEntryRequest,
        operation_id: &str,
        resumed: bool,
    ) -> Result<crate::domain::models::TimerId, TimeTrackingError> {
        if resumed {
            if let Some(entry_id) = self
                .client
                .find_time_entry_by_operation_id(operation_id, request.start_time.date())
                .await?
            {
                return Ok(entry_id);
            }
        }
        self.client.create_time_entry(request, operation_id).await
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
impl<C: TimeTrackingClient, R: TimerHistoryRepository> TimeTrackingService
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
        let idempotency_key = Self::validate_idempotency_key(idempotency_key)?;
        let operation = TimeTrackingWriteOperation::SaveActiveTimer;
        let request_hash = Self::hash_bytes(
            serde_json::to_string(&serde_json::json!({ "note": &note }))
                .map_err(|error| TimeTrackingError::unknown(error.to_string()))?
                .as_bytes(),
        );
        let claim = self
            .claim_write(user_id, operation, idempotency_key, &request_hash)
            .await?;
        let (operation_id, resumed) = match claim {
            ClaimedWrite::Acquired {
                operation_id,
                resumed,
            } => (operation_id, resumed),
            ClaimedWrite::Replay(entry) => return Ok(entry),
        };

        let active_timer = self
            .timer_repo
            .get_active_timer(user_id)
            .await?
            .ok_or(TimeTrackingError::NoTimerRunning);
        let active_timer = match active_timer {
            Ok(timer) => timer,
            Err(error) => {
                self.release_write(user_id, operation, idempotency_key)
                    .await;
                return Err(error);
            }
        };

        let end_time = OffsetDateTime::now_utc();
        if let Err(error) = Self::validate_interval(active_timer.started_at, end_time) {
            self.release_write(user_id, operation, idempotency_key)
                .await;
            return Err(error);
        }
        if let Err(error) = self
            .ensure_day_is_open(active_timer.started_at.date())
            .await
        {
            self.release_write(user_id, operation, idempotency_key)
                .await;
            return Err(error);
        }
        let selection = match (&active_timer.project_id, &active_timer.activity_id) {
            (Some(project_id), Some(activity_id)) => {
                self.resolve_required_selection(
                    project_id,
                    activity_id,
                    active_timer.started_at.date(),
                )
                .await
            }
            _ => Err(TimeTrackingError::InvalidProjectActivity(
                "a project and activity are required before saving a timer".to_string(),
            )),
        };
        let (project, activity) = match selection {
            Ok(selection) => selection,
            Err(error) => {
                self.release_write(user_id, operation, idempotency_key)
                    .await;
                return Err(error);
            }
        };
        let req = CreateTimeEntryRequest {
            project_id: project.id.clone(),
            activity_id: activity.id.clone(),
            start_time: active_timer.started_at,
            end_time,
            note: note.unwrap_or_else(|| active_timer.note.clone()),
        };

        let timer_id = match self
            .create_or_reconcile_entry(&req, &operation_id, resumed)
            .await
        {
            Ok(timer_id) => timer_id,
            Err(error) => {
                self.release_write(user_id, operation, idempotency_key)
                    .await;
                return Err(error);
            }
        };
        let created_entry =
            Self::time_entry_from_create_request(&req, &project, &activity, timer_id.to_string());

        self.timer_repo
            .finish_timer_idempotently(
                user_id,
                &end_time,
                timer_id.as_str(),
                idempotency_key,
                &created_entry,
            )
            .await?;

        Ok(created_entry)
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
        let idempotency_key = Self::validate_idempotency_key(idempotency_key)?;
        let operation = TimeTrackingWriteOperation::CreateTimeEntry;
        let request_hash = Self::hash_bytes(
            &serde_json::to_vec(request)
                .map_err(|error| TimeTrackingError::unknown(error.to_string()))?,
        );
        let claim = self
            .claim_write(user_id, operation, idempotency_key, &request_hash)
            .await?;
        let (operation_id, resumed) = match claim {
            ClaimedWrite::Acquired {
                operation_id,
                resumed,
            } => (operation_id, resumed),
            ClaimedWrite::Replay(entry) => return Ok(entry),
        };

        if let Err(error) = Self::validate_interval(request.start_time, request.end_time) {
            self.release_write(user_id, operation, idempotency_key)
                .await;
            return Err(error);
        }
        if let Err(error) = self.ensure_day_is_open(request.start_time.date()).await {
            self.release_write(user_id, operation, idempotency_key)
                .await;
            return Err(error);
        }
        let (project, activity) = match self
            .resolve_required_selection(
                &request.project_id,
                &request.activity_id,
                request.start_time.date(),
            )
            .await
        {
            Ok(selection) => selection,
            Err(error) => {
                self.release_write(user_id, operation, idempotency_key)
                    .await;
                return Err(error);
            }
        };

        let registration_id = match self
            .create_or_reconcile_entry(request, &operation_id, resumed)
            .await
        {
            Ok(registration_id) => registration_id,
            Err(error) => {
                self.release_write(user_id, operation, idempotency_key)
                    .await;
                return Err(error);
            }
        };
        let created_entry = Self::time_entry_from_create_request(
            request,
            &project,
            &activity,
            registration_id.to_string(),
        );

        // Persist to local timer history
        let entry = NewTimerHistoryEntry {
            user_id: *user_id,
            registration_id: registration_id.to_string(),
            start_time: request.start_time,
            end_time: request.end_time,
            project_id: Some(request.project_id.clone()),
            project_name: Some(project.name.clone()),
            activity_id: Some(request.activity_id.clone()),
            activity_name: Some(activity.name.clone()),
            note: request.note.clone(),
        };

        self.timer_repo
            .create_finished_idempotently(&entry, idempotency_key, &created_entry)
            .await?;

        Ok(created_entry)
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
mod tests {
    use super::*;
    use crate::domain::models::{CreateAbsenceDay, TimerHistoryId, TimerId};
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Mutex,
        },
    };
    use time::Duration;

    #[derive(Default)]
    struct MockTimeTrackingClient {
        created_request: Mutex<Option<CreateTimeEntryRequest>>,
        created_count: AtomicUsize,
        create_error: Mutex<Option<TimeTrackingError>>,
        found_entry: Mutex<Option<TimerId>>,
        existing_entry: Mutex<Option<TimeEntry>>,
        day_statuses: Vec<TimeEntryDayStatus>,
        absence_types: Vec<AbsenceType>,
        absence_children: Vec<AbsenceChild>,
    }

    #[async_trait]
    impl TimeTrackingClient for MockTimeTrackingClient {
        async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
            Ok(vec![Project::new("project-1", "Project")])
        }

        async fn get_activities(
            &self,
            _project_id: &ProjectId,
            _date_range: (Date, Date),
        ) -> Result<Vec<Activity>, TimeTrackingError> {
            Ok(vec![Activity::new("activity-1", "Activity", "project-1")])
        }

        async fn get_time_info(
            &self,
            _date_range: (Date, Date),
        ) -> Result<WeeklyStats, TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_time_entries(
            &self,
            _date_range: (Date, Date),
        ) -> Result<Vec<TimeEntry>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_time_entry_day_statuses(
            &self,
            _date_range: (Date, Date),
        ) -> Result<Vec<TimeEntryDayStatus>, TimeTrackingError> {
            Ok(self.day_statuses.clone())
        }

        async fn create_time_entry(
            &self,
            request: &CreateTimeEntryRequest,
            _operation_id: &str,
        ) -> Result<TimerId, TimeTrackingError> {
            self.created_count.fetch_add(1, Ordering::SeqCst);
            *self.created_request.lock().unwrap() = Some(request.clone());
            if let Some(error) = self.create_error.lock().unwrap().take() {
                return Err(error);
            }
            Ok(TimerId::new("entry-1"))
        }

        async fn find_time_entry_by_operation_id(
            &self,
            _operation_id: &str,
            _date: Date,
        ) -> Result<Option<TimerId>, TimeTrackingError> {
            Ok(self.found_entry.lock().unwrap().clone())
        }

        async fn get_time_entry(
            &self,
            _registration_id: &str,
        ) -> Result<TimeEntry, TimeTrackingError> {
            self.existing_entry
                .lock()
                .unwrap()
                .clone()
                .ok_or(TimeTrackingError::TimerNotFound)
        }

        async fn edit_time_entry(
            &self,
            _registration_id: &str,
            _request: &EditTimeEntryRequest,
        ) -> Result<TimerId, TimeTrackingError> {
            unused_mock_method()
        }

        async fn delete_time_entry(&self, _registration_id: &str) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_absence_types(&self) -> Result<Vec<AbsenceType>, TimeTrackingError> {
            Ok(self.absence_types.clone())
        }

        async fn get_absence_children(&self) -> Result<Vec<AbsenceChild>, TimeTrackingError> {
            Ok(self.absence_children.clone())
        }

        async fn get_absences(
            &self,
            _date_range: (Date, Date),
        ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_absence_day_defaults(
            &self,
            _date_range: (Date, Date),
        ) -> Result<Vec<AbsenceDayDefault>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn create_absences(
            &self,
            _request: &CreateAbsencesRequest,
        ) -> Result<Vec<AbsenceEntry>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn delete_absence(
            &self,
            _absence_id: &str,
            _date: Date,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }
    }

    #[derive(Default)]
    struct MockTimerHistoryRepository {
        active_timer: Mutex<Option<ActiveTimer>>,
        saved_end_time: Mutex<Option<OffsetDateTime>>,
        claims: Mutex<VecDeque<IdempotencyClaim>>,
        completed: Mutex<Vec<TimeEntry>>,
        releases: AtomicUsize,
        fail_save_once: AtomicBool,
    }

    #[async_trait]
    impl TimerHistoryRepository for MockTimerHistoryRepository {
        async fn get_active_timer(
            &self,
            _user_id: &UserId,
        ) -> Result<Option<ActiveTimer>, TimeTrackingError> {
            Ok(self.active_timer.lock().unwrap().clone())
        }

        async fn create_timer(
            &self,
            _user_id: &UserId,
            timer: &ActiveTimer,
        ) -> Result<(), TimeTrackingError> {
            let mut active = self.active_timer.lock().unwrap();
            if active.is_some() {
                return Err(TimeTrackingError::TimerAlreadyRunning);
            }
            *active = Some(timer.clone());
            Ok(())
        }

        async fn update_timer(
            &self,
            _user_id: &UserId,
            _timer: &ActiveTimer,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn delete_timer(&self, _user_id: &UserId) -> Result<(), TimeTrackingError> {
            *self.active_timer.lock().unwrap() = None;
            Ok(())
        }

        async fn finish_timer_idempotently(
            &self,
            _user_id: &UserId,
            end_time: &OffsetDateTime,
            _registration_id: &str,
            _key: &str,
            result: &TimeEntry,
        ) -> Result<(), TimeTrackingError> {
            if self.fail_save_once.swap(false, Ordering::SeqCst) {
                return Err(TimeTrackingError::unknown("simulated local failure"));
            }
            *self.saved_end_time.lock().unwrap() = Some(*end_time);
            *self.active_timer.lock().unwrap() = None;
            self.completed.lock().unwrap().push(result.clone());
            Ok(())
        }

        async fn get_history(
            &self,
            _user_id: &UserId,
        ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_by_registration_id(
            &self,
            _registration_id: &str,
        ) -> Result<Option<TimerHistoryEntry>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn create_finished_idempotently(
            &self,
            _entry: &NewTimerHistoryEntry,
            _key: &str,
            result: &TimeEntry,
        ) -> Result<TimerHistoryId, TimeTrackingError> {
            self.completed.lock().unwrap().push(result.clone());
            Ok(TimerHistoryId::new(1))
        }

        async fn update_times(
            &self,
            _registration_id: &str,
            _start_time: &OffsetDateTime,
            _end_time: &OffsetDateTime,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn update_registration_and_times(
            &self,
            _old_registration_id: &str,
            _new_registration_id: &str,
            _start_time: &OffsetDateTime,
            _end_time: &OffsetDateTime,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn claim_idempotency(
            &self,
            _user_id: &UserId,
            _operation: TimeTrackingWriteOperation,
            _key: &str,
            _request_hash: &str,
            operation_id: &str,
        ) -> Result<IdempotencyClaim, TimeTrackingError> {
            Ok(self
                .claims
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| IdempotencyClaim::Fresh {
                    operation_id: operation_id.to_string(),
                }))
        }

        async fn release_idempotency(
            &self,
            _user_id: &UserId,
            _operation: TimeTrackingWriteOperation,
            _key: &str,
        ) -> Result<(), TimeTrackingError> {
            self.releases.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn unused_mock_method<T>() -> Result<T, TimeTrackingError> {
        panic!("test called an unexpected mock method")
    }

    fn sample_create_request(date: Date) -> CreateTimeEntryRequest {
        let start_time = date.with_hms(9, 0, 0).unwrap().assume_utc();
        CreateTimeEntryRequest {
            project_id: ProjectId::new("project-1"),
            activity_id: crate::domain::models::ActivityId::new("activity-1"),
            start_time,
            end_time: start_time + Duration::hours(1),
            note: "Work".to_string(),
        }
    }

    fn sample_time_entry(date: Date, status: TimeEntryStatus) -> TimeEntry {
        let request = sample_create_request(date);
        TimeEntry::new(
            "entry-1",
            request.project_id,
            "Project",
            request.activity_id,
            "Activity",
            date,
            1.0,
        )
        .with_note(request.note)
        .with_times(Some(request.start_time), Some(request.end_time))
        .with_week_number(date.iso_week())
        .with_status(status)
    }

    #[tokio::test]
    async fn save_timer_uses_current_time_without_future_bonus() {
        let started_at = OffsetDateTime::now_utc() - Duration::minutes(20);
        let active_timer = ActiveTimer::new(started_at)
            .with_project("project-1", "Project")
            .with_activity("activity-1", "Activity")
            .with_note("note");
        let client = Arc::new(MockTimeTrackingClient::default());
        let repo = Arc::new(MockTimerHistoryRepository {
            active_timer: Mutex::new(Some(active_timer)),
            ..Default::default()
        });
        let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());
        let user_id = UserId::new(1);

        let before_save = OffsetDateTime::now_utc();
        let saved_entry = service
            .save_timer(&user_id, None, "save-test-1")
            .await
            .unwrap();
        let after_save = OffsetDateTime::now_utc();

        let provider_request = client.created_request.lock().unwrap().clone().unwrap();
        let history_end_time = repo.saved_end_time.lock().unwrap().unwrap();

        assert!(provider_request.end_time >= before_save);
        assert!(provider_request.end_time <= after_save);
        assert_eq!(history_end_time, provider_request.end_time);
        assert_eq!(saved_entry.end_time, Some(provider_request.end_time));
    }

    #[tokio::test]
    async fn idempotent_create_replays_the_original_result_without_provider_calls() {
        let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
        let expected = sample_time_entry(date, TimeEntryStatus::Open);
        let client = Arc::new(MockTimeTrackingClient::default());
        let repo = Arc::new(MockTimerHistoryRepository {
            claims: Mutex::new(VecDeque::from([IdempotencyClaim::Replay(expected.clone())])),
            ..Default::default()
        });
        let service = TimeTrackingServiceImpl::new(client.clone(), repo);

        let actual = service
            .create_time_entry(
                &UserId::new(1),
                &sample_create_request(date),
                "create-replay",
            )
            .await
            .unwrap();

        assert_eq!(actual, expected);
        assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn idempotency_payload_mismatch_is_a_conflict() {
        let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
        let client = Arc::new(MockTimeTrackingClient::default());
        let repo = Arc::new(MockTimerHistoryRepository {
            claims: Mutex::new(VecDeque::from([IdempotencyClaim::PayloadMismatch])),
            ..Default::default()
        });
        let service = TimeTrackingServiceImpl::new(client.clone(), repo);

        let error = service
            .create_time_entry(&UserId::new(1), &sample_create_request(date), "reused-key")
            .await
            .unwrap_err();

        assert!(matches!(error, TimeTrackingError::IdempotencyConflict));
        assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn create_rejects_locked_periods_before_provider_writes() {
        let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
        let client = Arc::new(MockTimeTrackingClient {
            day_statuses: vec![TimeEntryDayStatus {
                date,
                status: TimeEntryStatus::Approved,
            }],
            ..Default::default()
        });
        let repo = Arc::new(MockTimerHistoryRepository::default());
        let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());

        let error = service
            .create_time_entry(
                &UserId::new(1),
                &sample_create_request(date),
                "locked-create",
            )
            .await
            .unwrap_err();

        assert!(matches!(error, TimeTrackingError::LockedPeriod));
        assert_eq!(client.created_count.load(Ordering::SeqCst), 0);
        assert_eq!(repo.releases.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn save_recovers_provider_success_after_local_failure_without_duplicate_creation() {
        let started_at = OffsetDateTime::now_utc() - Duration::minutes(20);
        let active_timer = ActiveTimer::new(started_at)
            .with_project("project-1", "Project")
            .with_activity("activity-1", "Activity")
            .with_note("note");
        let operation_id = "toki-op-crash-window".to_string();
        let client = Arc::new(MockTimeTrackingClient {
            found_entry: Mutex::new(Some(TimerId::new("entry-1"))),
            ..Default::default()
        });
        let repo = Arc::new(MockTimerHistoryRepository {
            active_timer: Mutex::new(Some(active_timer)),
            claims: Mutex::new(VecDeque::from([
                IdempotencyClaim::Fresh {
                    operation_id: operation_id.clone(),
                },
                IdempotencyClaim::Resumed { operation_id },
            ])),
            fail_save_once: AtomicBool::new(true),
            ..Default::default()
        });
        let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());
        let user_id = UserId::new(1);

        let first_error = service
            .save_timer(&user_id, None, "save-crash-window")
            .await
            .unwrap_err();
        assert!(matches!(first_error, TimeTrackingError::Unknown(_)));

        let recovered = service
            .save_timer(&user_id, None, "save-crash-window")
            .await
            .unwrap();

        assert_eq!(recovered.registration_id, "entry-1");
        assert_eq!(client.created_count.load(Ordering::SeqCst), 1);
        assert_eq!(repo.completed.lock().unwrap().len(), 1);
        assert_eq!(repo.releases.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn concurrent_starts_leave_exactly_one_active_timer() {
        let client = Arc::new(MockTimeTrackingClient::default());
        let repo = Arc::new(MockTimerHistoryRepository::default());
        let service = TimeTrackingServiceImpl::new(client, repo.clone());
        let user_id = UserId::new(1);
        let request = StartTimerRequest {
            project_id: None,
            activity_id: None,
            note: String::new(),
        };

        let (first, second) = tokio::join!(
            service.start_timer(&user_id, &request),
            service.start_timer(&user_id, &request)
        );

        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(matches!(
            first.err().or_else(|| second.err()),
            Some(TimeTrackingError::TimerAlreadyRunning)
        ));
        assert!(repo.active_timer.lock().unwrap().is_some());
    }

    #[tokio::test]
    async fn timer_selection_rejects_an_activity_without_a_project() {
        let service = TimeTrackingServiceImpl::new(
            Arc::new(MockTimeTrackingClient::default()),
            Arc::new(MockTimerHistoryRepository::default()),
        );
        let request = StartTimerRequest {
            project_id: None,
            activity_id: Some(crate::domain::models::ActivityId::new("activity-1")),
            note: String::new(),
        };

        let error = service
            .start_timer(&UserId::new(1), &request)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            TimeTrackingError::InvalidProjectActivity(_)
        ));
    }

    #[tokio::test]
    async fn delete_rejects_locked_owned_entries() {
        let date = Date::from_calendar_date(2026, time::Month::August, 25).unwrap();
        let client = Arc::new(MockTimeTrackingClient {
            existing_entry: Mutex::new(Some(sample_time_entry(date, TimeEntryStatus::Approved))),
            ..Default::default()
        });
        let service =
            TimeTrackingServiceImpl::new(client, Arc::new(MockTimerHistoryRepository::default()));

        let error = service.delete_time_entry("entry-1").await.unwrap_err();

        assert!(matches!(error, TimeTrackingError::LockedPeriod));
    }

    #[test]
    fn create_absences_validation_rejects_invalid_requests() {
        let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
        let valid_day = CreateAbsenceDay { date, hours: 8.0 };
        let valid = CreateAbsencesRequest {
            absence_type: AbsenceType::Vacation,
            child: None,
            comment: String::new(),
            days: vec![valid_day.clone()],
        };

        assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&valid).is_ok());

        let mut invalid = valid.clone();
        invalid.days = Vec::new();
        assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

        let mut invalid = valid.clone();
        invalid.days = vec![valid_day.clone(), valid_day.clone()];
        assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

        for hours in [0.0, -1.0, f64::NAN, 24.25] {
            let mut invalid = valid.clone();
            invalid.days = vec![CreateAbsenceDay { date, hours }];
            assert!(matches!(
                TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
                Err(TimeTrackingError::InvalidInput(_))
            ));
        }

        let invalid = CreateAbsencesRequest {
            absence_type: AbsenceType::ParentalLeave,
            child: Some(" ".to_string()),
            comment: String::new(),
            days: vec![valid_day.clone()],
        };
        assert!(matches!(
            TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&invalid),
            Err(TimeTrackingError::InvalidInput(_))
        ));

        let valid = CreateAbsencesRequest {
            absence_type: AbsenceType::Furlough,
            child: None,
            comment: String::new(),
            days: vec![valid_day],
        };
        assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_create_absences(&valid).is_ok());
    }

    #[tokio::test]
    async fn create_absences_rejects_unavailable_absence_type() {
        let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
        let client = Arc::new(MockTimeTrackingClient {
            created_request: Mutex::new(None),
            absence_types: vec![AbsenceType::Vacation],
            absence_children: Vec::new(),
            ..Default::default()
        });
        let repo = Arc::new(MockTimerHistoryRepository {
            active_timer: Mutex::new(None),
            ..Default::default()
        });
        let service = TimeTrackingServiceImpl::new(client, repo);

        let error = service
            .create_absences(&CreateAbsencesRequest {
                absence_type: AbsenceType::Sick,
                child: None,
                comment: String::new(),
                days: vec![CreateAbsenceDay { date, hours: 8.0 }],
            })
            .await
            .unwrap_err();

        assert!(
            matches!(error, TimeTrackingError::InvalidInput(message) if message == "This absence type is not available in Kleer")
        );
    }

    #[test]
    fn registered_child_validation_requires_matching_child() {
        let date = Date::from_calendar_date(2026, time::Month::May, 20).unwrap();
        let request = CreateAbsencesRequest {
            absence_type: AbsenceType::Childcare,
            child: Some("Barn1".to_string()),
            comment: String::new(),
            days: vec![CreateAbsenceDay { date, hours: 8.0 }],
        };
        let children = vec![AbsenceChild {
            name: "Barn1".to_string(),
            birth_date: Some(date),
        }];

        let child = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::trimmed_child_name(&request).unwrap();
        assert!(TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(child, &children).is_ok());

        let error = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(child, &[])
            .unwrap_err();
        assert!(
            matches!(error, TimeTrackingError::InvalidInput(message) if message == "Register a child in Kleer before reporting this absence type")
        );

        let error = TimeTrackingServiceImpl::<MockTimeTrackingClient, MockTimerHistoryRepository>::validate_registered_child(
            "Other",
            &children,
        )
        .unwrap_err();
        assert!(
            matches!(error, TimeTrackingError::InvalidInput(message) if message == "Select a registered child for this absence type")
        );
    }
}
