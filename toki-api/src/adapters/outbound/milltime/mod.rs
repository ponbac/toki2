mod conversions;

use async_trait::async_trait;
use chrono::Datelike;
use time::Date;

use crate::domain::{
    models::{
        Activity, ActiveTimer, CreateTimeEntryRequest, EditTimeEntryRequest, Project, ProjectId,
        StartTimerRequest, TimeEntry, TimeInfo, TimerId,
    },
    ports::outbound::TimeTrackingClient,
    TimeTrackingError,
};

use self::conversions::{
    to_domain_active_timer, to_domain_activity, to_domain_project, to_domain_time_entry,
    to_domain_time_info,
};

/// Adapter that wraps the Milltime client to implement the TimeTrackingClient port.
pub struct MilltimeAdapter {
    client: milltime::MilltimeClient,
}

impl MilltimeAdapter {
    /// Create a new MilltimeAdapter with authenticated credentials.
    pub fn new(credentials: milltime::Credentials) -> Self {
        Self {
            client: milltime::MilltimeClient::new(credentials),
        }
    }

    /// Get the Milltime user ID (for internal use).
    pub fn milltime_user_id(&self) -> &str {
        self.client.user_id()
    }
}

#[async_trait]
impl TimeTrackingClient for MilltimeAdapter {
    async fn get_active_timer(&self) -> Result<Option<ActiveTimer>, TimeTrackingError> {
        match self.client.fetch_timer().await {
            Ok(timer) => Ok(Some(to_domain_active_timer(timer))),
            Err(milltime::MilltimeFetchError::ParsingError(msg)) if msg.contains("Expected exactly one row, got 0") => {
                Ok(None)
            }
            Err(e) => Err(map_milltime_error(e)),
        }
    }

    async fn start_timer(&self, req: &StartTimerRequest) -> Result<ActiveTimer, TimeTrackingError> {
        let mt_user_id = self.client.user_id().to_string();

        let options = milltime::StartTimerOptions::new(
            req.activity_id.as_str().to_string(),
            req.activity_name.clone(),
            req.project_id.as_str().to_string(),
            req.project_name.clone(),
            mt_user_id,
            req.note.clone(),
            req.reg_day.clone(),
            req.week_number,
            None,
            None,
        );

        self.client.start_timer(options).await.map_err(map_milltime_error)?;

        // Fetch the timer we just started to return it
        let timer = self.client.fetch_timer().await.map_err(map_milltime_error)?;
        Ok(to_domain_active_timer(timer))
    }

    async fn stop_timer(&self) -> Result<(), TimeTrackingError> {
        self.client.stop_timer().await.map_err(map_milltime_error)
    }

    async fn save_timer(&self, note: Option<&str>) -> Result<TimerId, TimeTrackingError> {
        let payload = milltime::SaveTimerPayload {
            user_note: note.map(String::from),
        };

        let result = self.client.save_timer(payload).await.map_err(map_milltime_error)?;
        Ok(TimerId::new(result.project_registration_id))
    }

    async fn get_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        let filter = milltime::ProjectSearchFilter::new("Overview".to_string());
        let projects = self.client.fetch_project_search(filter).await.map_err(map_milltime_error)?;
        // Filter to only show projects where user is a member
        Ok(projects.into_iter()
            .filter(|p| p.is_member)
            .map(to_domain_project)
            .collect())
    }

    async fn get_activities(
        &self,
        project_id: &ProjectId,
        date_range: (Date, Date),
    ) -> Result<Vec<Activity>, TimeTrackingError> {
        let filter = milltime::ActivityFilter::new(
            project_id.as_str().to_string(),
            date_range.0.to_string(),
            date_range.1.to_string(),
        );
        let activities = self.client.fetch_activities(filter).await.map_err(map_milltime_error)?;
        Ok(activities.into_iter().map(|a| to_domain_activity(a, project_id)).collect())
    }

    async fn get_time_info(
        &self,
        date_range: (Date, Date),
    ) -> Result<TimeInfo, TimeTrackingError> {
        let date_filter: milltime::DateFilter =
            format!("{},{}", date_range.0, date_range.1)
                .parse()
                .map_err(|_| TimeTrackingError::InvalidDateRange)?;

        let info = self.client.fetch_time_info(date_filter).await.map_err(map_milltime_error)?;
        Ok(to_domain_time_info(info))
    }

    async fn get_time_entries(
        &self,
        date_range: (Date, Date),
    ) -> Result<Vec<TimeEntry>, TimeTrackingError> {
        let date_filter: milltime::DateFilter =
            format!("{},{}", date_range.0, date_range.1)
                .parse()
                .map_err(|_| TimeTrackingError::InvalidDateRange)?;

        let calendar = self.client.fetch_user_calendar(&date_filter).await.map_err(map_milltime_error)?;

        // Flatten weeks -> days -> time_entries, filtering by date range
        let entries: Vec<TimeEntry> = calendar
            .weeks
            .into_iter()
            .flat_map(|week| week.days)
            .filter(|day| {
                // Convert chrono::NaiveDate to time::Date for comparison
                let day_date = time::Date::from_calendar_date(
                    day.date.year(),
                    time::Month::try_from(day.date.month() as u8).unwrap_or(time::Month::January),
                    day.date.day() as u8,
                )
                .unwrap_or(time::Date::from_calendar_date(1970, time::Month::January, 1).unwrap());

                day_date >= date_range.0 && day_date <= date_range.1
            })
            .flat_map(|day| day.time_entries)
            .map(to_domain_time_entry)
            .collect();

        Ok(entries)
    }

    async fn create_time_entry(
        &self,
        request: &CreateTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        let total_time = format!(
            "{:02}:{:02}",
            (request.end_time - request.start_time).whole_hours(),
            (request.end_time - request.start_time).whole_minutes() % 60
        );

        let payload = milltime::ProjectRegistrationPayload::new(
            self.client.user_id().to_string(),
            request.project_id.as_str().to_string(),
            request.project_name.clone(),
            request.activity_id.as_str().to_string(),
            request.activity_name.clone(),
            total_time,
            request.reg_day.clone(),
            request.week_number,
            request.note.clone(),
        );

        let response = self.client.new_project_registration(&payload).await.map_err(map_milltime_error)?;
        Ok(TimerId::new(response.project_registration_id))
    }

    async fn edit_time_entry(
        &self,
        request: &EditTimeEntryRequest,
    ) -> Result<TimerId, TimeTrackingError> {
        let total_time = format!(
            "{:02}:{:02}",
            (request.end_time - request.start_time).whole_hours(),
            (request.end_time - request.start_time).whole_minutes() % 60
        );

        let regday_changed = request
            .original_reg_day
            .as_ref()
            .map(|orig| *orig != request.reg_day)
            .unwrap_or(false);

        if regday_changed {
            // Create new registration with new day
            let new_payload = milltime::ProjectRegistrationPayload::new(
                self.client.user_id().to_string(),
                request.project_id.as_str().to_string(),
                request.project_name.clone(),
                request.activity_id.as_str().to_string(),
                request.activity_name.clone(),
                total_time,
                request.reg_day.clone(),
                request.week_number,
                request.note.clone(),
            );

            let new_registration = self
                .client
                .new_project_registration(&new_payload)
                .await
                .map_err(map_milltime_error)?;

            // Delete old registration
            self.client
                .delete_project_registration(request.registration_id.clone())
                .await
                .map_err(map_milltime_error)?;

            Ok(TimerId::new(new_registration.project_registration_id))
        } else {
            // Day unchanged -> regular edit
            let payload = milltime::ProjectRegistrationEditPayload::new(
                request.registration_id.clone(),
                self.client.user_id().to_string(),
                request.project_id.as_str().to_string(),
                request.project_name.clone(),
                request.activity_id.as_str().to_string(),
                request.activity_name.clone(),
                total_time,
                request.reg_day.clone(),
                request.week_number,
                request.note.clone(),
            );

            self.client
                .edit_project_registration(&payload)
                .await
                .map_err(map_milltime_error)?;

            Ok(TimerId::new(request.registration_id.clone()))
        }
    }

    async fn delete_time_entry(&self, registration_id: &str) -> Result<(), TimeTrackingError> {
        self.client
            .delete_project_registration(registration_id.to_string())
            .await
            .map_err(map_milltime_error)
    }
}

fn map_milltime_error(e: milltime::MilltimeFetchError) -> TimeTrackingError {
    match e {
        milltime::MilltimeFetchError::Unauthorized => TimeTrackingError::AuthenticationFailed,
        milltime::MilltimeFetchError::ResponseError(msg) => TimeTrackingError::unknown(msg),
        milltime::MilltimeFetchError::ParsingError(msg) => TimeTrackingError::unknown(msg),
        milltime::MilltimeFetchError::Other(msg) => TimeTrackingError::unknown(msg),
    }
}
