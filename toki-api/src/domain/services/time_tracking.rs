use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use itertools::Itertools;
use time::Date;

use crate::domain::{
    models::{
        Activity, ActiveTimer, CreateTimeEntryRequest, EditTimeEntryRequest,
        NewTimerHistoryEntry, Project, ProjectId, StartTimerRequest, TimeEntry, TimeInfo,
        TimerId, TimerSource, UserId,
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
/// Optionally takes a TimerHistoryRepository to merge local timer history
/// with provider data.
pub struct TimeTrackingServiceImpl<C, R = ()> {
    client: Arc<C>,
    timer_repo: Option<Arc<R>>,
}

impl<C> TimeTrackingServiceImpl<C, ()> {
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            timer_repo: None,
        }
    }
}

impl<C, R> TimeTrackingServiceImpl<C, R> {
    pub fn with_timer_repo(client: Arc<C>, timer_repo: Arc<R>) -> Self {
        Self {
            client,
            timer_repo: Some(timer_repo),
        }
    }
}

#[async_trait]
impl<C: TimeTrackingClient, R: TimerHistoryRepository> TimeTrackingService
    for TimeTrackingServiceImpl<C, R>
{
    async fn get_active_timer(&self) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        self.client.get_active_timer().await
    }

    async fn start_timer(&self, req: &StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError> {
        // Business logic: Check if a timer is already running
        if self.client.get_active_timer().await?.is_some() {
            return Err(TimeTrackingError::TimerAlreadyRunning);
        }

        self.client.start_timer(req).await
    }

    async fn stop_timer(&self) -> Result<(), TimeTrackingError> {
        self.client.stop_timer().await
    }

    async fn save_timer(&self, note: Option<&str>) -> Result<TimerId, TimeTrackingError> {
        self.client.save_timer(note).await
    }

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

    async fn get_time_info(
        &self,
        date_range: (Date, Date),
    ) -> Result<TimeInfo, TimeTrackingError> {
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

        // Merge with local timer history if available
        if let Some(repo) = &self.timer_repo {
            let history = repo.get_history(user_id).await?;

            // Build a map of registration_id -> (start_time, end_time)
            let history_map: HashMap<String, _> = history
                .into_iter()
                .filter_map(|h| h.registration_id.map(|reg_id| (reg_id, (h.start_time, h.end_time))))
                .collect();

            // Augment entries with local start/end times
            for entry in &mut entries {
                if let Some((start_time, end_time)) = history_map.get(&entry.registration_id) {
                    entry.start_time = Some(*start_time);
                    entry.end_time = *end_time;
                }
            }
        }

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

    async fn create_time_entry(
        &self,
        user_id: &UserId,
        request: &CreateTimeEntryRequest,
    ) -> Result<(), TimeTrackingError> {
        // Create in provider
        let registration_id = self.client.create_time_entry(request).await?;

        // Persist to local timer history if available
        if let Some(repo) = &self.timer_repo {
            let entry = NewTimerHistoryEntry {
                user_id: *user_id,
                source: TimerSource::Standalone, // Manual entries are "standalone" type
                registration_id: registration_id.to_string(),
                start_time: request.start_time,
                end_time: request.end_time,
                project_id: Some(request.project_id.clone()),
                project_name: Some(request.project_name.clone()),
                activity_id: Some(request.activity_id.clone()),
                activity_name: Some(request.activity_name.clone()),
                note: request.note.clone(),
            };

            if let Err(e) = repo.create_finished(&entry).await {
                tracing::error!("Failed to persist timer to local history: {:?}", e);
                // Don't fail the request - the provider entry was created successfully
            }
        }

        Ok(())
    }

    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<(), TimeTrackingError> {
        // Edit in provider (may return a new registration ID if day changed)
        let new_registration_id = self.client.edit_time_entry(request).await?;

        // Update local timer history if available
        if let Some(repo) = &self.timer_repo {
            // Check if we have a local record for this registration
            if repo
                .get_by_registration_id(&request.registration_id)
                .await?
                .is_some()
            {
                // Check if registration ID changed (day changed)
                if new_registration_id.as_str() != request.registration_id {
                    // Update both registration ID and times
                    if let Err(e) = repo
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
                    if let Err(e) = repo
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
        }

        Ok(())
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        // Delete from provider
        self.client.delete_time_entry(registration_id).await
        // Note: We don't delete from local timer history - it serves as an audit log
    }
}
