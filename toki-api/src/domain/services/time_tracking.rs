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
        let end_time = OffsetDateTime::now_utc();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{TimerHistoryId, TimerId};
    use std::sync::Mutex;
    use time::Duration;

    #[derive(Default)]
    struct MockTimeTrackingClient {
        created_request: Mutex<Option<CreateTimeEntryRequest>>,
    }

    #[async_trait]
    impl TimeTrackingClient for MockTimeTrackingClient {
        async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
            unused_mock_method()
        }

        async fn get_activities(
            &self,
            _project_id: &ProjectId,
            _date_range: (Date, Date),
        ) -> Result<Vec<Activity>, TimeTrackingError> {
            unused_mock_method()
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
            unused_mock_method()
        }

        async fn create_time_entry(
            &self,
            request: &CreateTimeEntryRequest,
        ) -> Result<TimerId, TimeTrackingError> {
            *self.created_request.lock().unwrap() = Some(request.clone());
            Ok(TimerId::new("entry-1"))
        }

        async fn edit_time_entry(
            &self,
            _request: &EditTimeEntryRequest,
        ) -> Result<TimerId, TimeTrackingError> {
            unused_mock_method()
        }

        async fn delete_time_entry(&self, _registration_id: &str) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }
    }

    struct MockTimerHistoryRepository {
        active_timer: Mutex<Option<ActiveTimer>>,
        saved_end_time: Mutex<Option<OffsetDateTime>>,
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
            _timer: &ActiveTimer,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn update_timer(
            &self,
            _user_id: &UserId,
            _timer: &ActiveTimer,
        ) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn delete_timer(&self, _user_id: &UserId) -> Result<(), TimeTrackingError> {
            unused_mock_method()
        }

        async fn save_timer_finished(
            &self,
            _user_id: &UserId,
            end_time: &OffsetDateTime,
            _registration_id: &str,
        ) -> Result<(), TimeTrackingError> {
            *self.saved_end_time.lock().unwrap() = Some(*end_time);
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

        async fn create_finished(
            &self,
            _entry: &NewTimerHistoryEntry,
        ) -> Result<TimerHistoryId, TimeTrackingError> {
            unused_mock_method()
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
    }

    fn unused_mock_method<T>() -> Result<T, TimeTrackingError> {
        panic!("test called an unexpected mock method")
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
            saved_end_time: Mutex::new(None),
        });
        let service = TimeTrackingServiceImpl::new(client.clone(), repo.clone());
        let user_id = UserId::new(1);

        let before_save = OffsetDateTime::now_utc();
        let saved_entry = service.save_timer(&user_id, None).await.unwrap();
        let after_save = OffsetDateTime::now_utc();

        let provider_request = client.created_request.lock().unwrap().clone().unwrap();
        let history_end_time = repo.saved_end_time.lock().unwrap().unwrap();

        assert!(provider_request.end_time >= before_save);
        assert!(provider_request.end_time <= after_save);
        assert_eq!(history_end_time, provider_request.end_time);
        assert_eq!(saved_entry.end_time, Some(provider_request.end_time));
    }
}
