use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use itertools::Itertools;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        ActiveTimer, Activity, CreateTimeEntryRequest, EditTimeEntryRequest, NewTimerHistoryEntry,
        Project, ProjectId, TimeEntry, TimeEntryDayStatus, TimeEntryStatus, TimerHistoryEntry,
        UserId, WeeklyStats,
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

impl<C, R> TimeTrackingServiceImpl<C, R> {
    pub fn new(client: Arc<C>, timer_repo: Arc<R>) -> Self {
        Self { client, timer_repo }
    }

    fn time_entry_from_create_request(
        request: &CreateTimeEntryRequest,
        registration_id: impl Into<String>,
    ) -> TimeEntry {
        let date = request.start_time.date();
        let hours = (request.end_time - request.start_time).whole_seconds() as f64 / 3600.0;

        TimeEntry::new(
            registration_id,
            request.project_id.clone(),
            request.project_name.clone(),
            request.activity_id.clone(),
            request.activity_name.clone(),
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
        registration_id: impl Into<String>,
    ) -> TimeEntry {
        let date = request.start_time.date();
        let hours = (request.end_time - request.start_time).whole_seconds() as f64 / 3600.0;

        TimeEntry::new(
            registration_id,
            request.project_id.clone(),
            request.project_name.clone(),
            request.activity_id.clone(),
            request.activity_name.clone(),
            date,
            hours,
        )
        .with_note(request.note.clone())
        .with_times(Some(request.start_time), Some(request.end_time))
        .with_week_number(date.iso_week())
        .with_status(TimeEntryStatus::Open)
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
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        // Business logic: Check if a timer is already running
        if self.timer_repo.get_active_timer(user_id).await?.is_some() {
            return Err(TimeTrackingError::TimerAlreadyRunning);
        }

        self.timer_repo.create_timer(user_id, timer).await
    }

    async fn stop_timer(&self, user_id: &UserId) -> Result<(), TimeTrackingError> {
        self.timer_repo.delete_timer(user_id).await
    }

    async fn save_timer(
        &self,
        user_id: &UserId,
        note: Option<String>,
    ) -> Result<TimeEntry, TimeTrackingError> {
        // Get the active timer
        let active_timer = self
            .timer_repo
            .get_active_timer(user_id)
            .await?
            .ok_or(TimeTrackingError::NoTimerRunning)?;

        // Compute times
        const BONUS_TIME_MINUTES: i64 = 1;
        let now = OffsetDateTime::now_utc();
        let end_time = now + time::Duration::minutes(BONUS_TIME_MINUTES);

        // Build the create request
        let req = CreateTimeEntryRequest {
            project_id: active_timer
                .project_id
                .clone()
                .ok_or_else(|| TimeTrackingError::unknown("project id not set on timer"))?,
            project_name: active_timer
                .project_name
                .clone()
                .ok_or_else(|| TimeTrackingError::unknown("project name not set on timer"))?,
            activity_id: active_timer
                .activity_id
                .clone()
                .ok_or_else(|| TimeTrackingError::unknown("activity id not set on timer"))?,
            activity_name: active_timer
                .activity_name
                .clone()
                .ok_or_else(|| TimeTrackingError::unknown("activity name not set on timer"))?,
            start_time: active_timer.started_at,
            end_time,
            note: note.unwrap_or_else(|| active_timer.note.clone()),
        };

        // Create time entry in the provider
        let timer_id = self.client.create_time_entry(&req).await?;
        let created_entry = Self::time_entry_from_create_request(&req, timer_id.to_string());

        // Mark the active timer as finished
        self.timer_repo
            .save_timer_finished(user_id, &end_time, timer_id.as_str())
            .await?;

        Ok(created_entry)
    }

    async fn edit_timer(
        &self,
        user_id: &UserId,
        timer: &ActiveTimer,
    ) -> Result<(), TimeTrackingError> {
        self.timer_repo.update_timer(user_id, timer).await
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
    ) -> Result<TimeEntry, TimeTrackingError> {
        // Create in provider
        let registration_id = self.client.create_time_entry(request).await?;
        let created_entry =
            Self::time_entry_from_create_request(request, registration_id.to_string());

        // Persist to local timer history
        let entry = NewTimerHistoryEntry {
            user_id: *user_id,
            registration_id: registration_id.to_string(),
            start_time: request.start_time,
            end_time: request.end_time,
            project_id: Some(request.project_id.clone()),
            project_name: Some(request.project_name.clone()),
            activity_id: Some(request.activity_id.clone()),
            activity_name: Some(request.activity_name.clone()),
            note: request.note.clone(),
        };

        if let Err(e) = self.timer_repo.create_finished(&entry).await {
            tracing::error!("Failed to persist timer to local history: {:?}", e);
            // Don't fail the request - the provider entry was created successfully
        }

        Ok(created_entry)
    }

    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<TimeEntry, TimeTrackingError> {
        // Edit in provider (may return a new registration ID if day changed)
        let new_registration_id = self.client.edit_time_entry(request).await?;
        let updated_entry =
            Self::time_entry_from_edit_request(request, new_registration_id.to_string());

        // Update local timer history
        // Check if we have a local record for this registration
        if self
            .timer_repo
            .get_by_registration_id(&request.registration_id)
            .await?
            .is_some()
        {
            // Check if registration ID changed (day changed)
            if new_registration_id.as_str() != request.registration_id {
                // Update both registration ID and times
                if let Err(e) = self
                    .timer_repo
                    .update_registration_and_times(
                        &request.registration_id,
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
                    .update_times(
                        &request.registration_id,
                        &request.start_time,
                        &request.end_time,
                    )
                    .await
                {
                    tracing::error!("Failed to update timer times: {:?}", e);
                }
            }
        }

        Ok(updated_entry)
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        // Delete from provider
        self.client.delete_time_entry(registration_id).await
        // Note: We don't delete from local timer history - it serves as an audit log
    }

    async fn get_timer_history(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<TimerHistoryEntry>, TimeTrackingError> {
        self.timer_repo.get_history(user_id).await
    }
}
