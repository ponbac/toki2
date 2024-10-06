use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::{
        self, ActivityFilter, DateFilter, ProjectSearchFilter, TimerRegistrationFilter,
        TimerRegistrationPayload,
    },
    milltime_url::MilltimeURL,
    UpdateTimerFilter,
};

use super::Credentials;

pub struct MilltimeClient {
    credentials: Credentials,
}

impl MilltimeClient {
    pub fn new(credentials: Credentials) -> Self {
        Self { credentials }
    }

    pub fn user_id(&self) -> &str {
        &self.credentials.user_id
    }

    async fn get<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let client = reqwest::Client::new();

        let resp = client
            .get(url.as_ref())
            .milltime_headers(&self.credentials)
            .send()
            .await
            .map_err(|e| MilltimeFetchError::ResponseError(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(MilltimeFetchError::Unauthorized);
        }

        let resp_text = resp.text().await.map_err(|e| {
            MilltimeFetchError::ParsingError(format!("Failed to get response text: {}", e))
        })?;

        let resp_data = serde_json::from_str::<T>(&resp_text).map_err(|e| {
            let error_position = resp_text
                .lines()
                .take(e.line() - 1)
                .map(|line| line.len() + 1) // +1 for newline character
                .sum::<usize>()
                + e.column();

            let start = error_position.saturating_sub(50);
            let end = usize::min(error_position + 50, resp_text.len());
            let error_context = resp_text[start..end].to_string();

            MilltimeFetchError::ParsingError(format!(
                "Failed to parse response as JSON: {}. Context: {}",
                e, error_context
            ))
        })?;

        Ok(resp_data)
    }

    async fn get_single_row<T: DeserializeOwned + Serialize>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let response: MilltimeRowResponse<T> = self.get(url).await?;
        match response.success {
            true => response.only_row(),
            false => Err(MilltimeFetchError::Other(
                "milltime responded with success=false".to_string(),
            )),
        }
    }

    async fn post<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        payload: impl serde::Serialize,
    ) -> Result<T, MilltimeFetchError> {
        let client = reqwest::Client::new();

        let resp = client
            .post(url.as_ref())
            .milltime_headers(&self.credentials)
            .json(&payload)
            .send()
            .await
            .map_err(|e| MilltimeFetchError::ResponseError(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(MilltimeFetchError::Unauthorized);
        }

        let resp_data = resp.json::<T>().await.map_err(|e| {
            MilltimeFetchError::ParsingError(format!("Failed to parse response as JSON: {}", e))
        })?;

        Ok(resp_data)
    }

    async fn put<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        payload: Option<impl serde::Serialize>,
    ) -> Result<T, MilltimeFetchError> {
        let mut client = reqwest::Client::new()
            .put(url.as_ref())
            .milltime_headers(&self.credentials);

        if let Some(payload) = payload {
            client = client.json(&payload);
        }

        let resp = client
            .send()
            .await
            .map_err(|e| MilltimeFetchError::ResponseError(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(MilltimeFetchError::Unauthorized);
        }

        let resp_data = resp.json::<T>().await.map_err(|e| {
            MilltimeFetchError::ParsingError(format!("Failed to parse response as JSON: {}", e))
        })?;

        Ok(resp_data)
    }

    async fn delete<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let client = reqwest::Client::new();

        let resp = client
            .delete(url.as_ref())
            .milltime_headers(&self.credentials)
            .send()
            .await
            .map_err(|e| MilltimeFetchError::ResponseError(e.to_string()))?;

        if resp.status() == 401 || resp.status() == 403 {
            return Err(MilltimeFetchError::Unauthorized);
        }

        let resp_data = resp.json::<T>().await.map_err(|e| {
            MilltimeFetchError::ParsingError(format!("Failed to parse response as JSON: {}", e))
        })?;

        Ok(resp_data)
    }

    pub async fn fetch_time_period_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimePeriodInfo, MilltimeFetchError> {
        let url = MilltimeURL::from_env()
            .append_path("/data/store/TimeInfo")
            .with_filter(&date_filter);

        let time_period_info = self.get_single_row::<domain::TimePeriodInfo>(url).await?;

        Ok(domain::TimePeriodInfo {
            from: date_filter.from,
            to: date_filter.to,
            ..time_period_info
        })
    }

    pub async fn fetch_user_calendar(
        &self,
        date_filter: &DateFilter,
    ) -> Result<domain::UserCalendar, MilltimeFetchError> {
        let url = MilltimeURL::from_env()
            .append_path("/data/store/UserCalendar")
            .with_filter(date_filter);

        let raw_calendar = self.get_single_row::<domain::RawUserCalendar>(url).await?;

        let transformed_weeks = raw_calendar
            .weeks
            .into_iter()
            .map(domain::Week::from)
            .collect();

        Ok(domain::UserCalendar {
            weeks: transformed_weeks,
        })
    }

    pub async fn fetch_project_search(
        &self,
        search_filter: ProjectSearchFilter,
    ) -> Result<Vec<domain::ProjectSearchItem>, MilltimeFetchError> {
        let url = MilltimeURL::from_env()
            .append_path("/data/store/ProjectSearchMT")
            .with_filter(&search_filter);

        let project_search = self
            .get::<MilltimeRowResponse<domain::ProjectSearchItem>>(url)
            .await?;

        Ok(project_search.rows)
    }

    pub async fn fetch_activities(
        &self,
        activity_filter: ActivityFilter,
    ) -> Result<Vec<domain::Activity>, MilltimeFetchError> {
        let url = MilltimeURL::from_env()
            .append_path("/data/store/ProjectPhaseActivity")
            .with_filter(&activity_filter);

        let root = self.get_single_row::<domain::ActivitiesRoot>(url).await?;

        Ok(root.activities)
    }

    pub async fn fetch_time_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimeInfo, MilltimeFetchError> {
        let url = MilltimeURL::from_env()
            .append_path("/data/store/TimeInfo")
            .with_filter(&date_filter);

        let time_info = self.get_single_row::<domain::TimeInfo>(url).await?;

        Ok(time_info)
    }

    pub async fn fetch_timer(&self) -> Result<domain::TimerRegistration, MilltimeFetchError> {
        let url = MilltimeURL::from_env().append_path("/data/store/TimerRegistration");

        let timer = self
            .get_single_row::<domain::TimerRegistration>(url)
            .await?;

        Ok(timer)
    }

    pub async fn start_timer(
        &self,
        start_timer_options: domain::StartTimerOptions,
    ) -> Result<(), MilltimeFetchError> {
        let payload: TimerRegistrationPayload = start_timer_options.into();
        let reg_timer_url_filter: TimerRegistrationFilter = (&payload).into();
        let url = MilltimeURL::from_env()
            .append_path("/data/store/TimerRegistration")
            .with_filter(&reg_timer_url_filter);

        match self
            .post::<MilltimeRowResponse<serde_json::Value>>(url, payload)
            .await?
        {
            MilltimeRowResponse { success: true, .. } => Ok(()),
            MilltimeRowResponse { success: false, .. } => Err(MilltimeFetchError::Other(
                "milltime responded with success=false".to_string(),
            )),
        }
    }

    pub async fn stop_timer(&self) -> Result<(), MilltimeFetchError> {
        let url = MilltimeURL::from_env().append_path("/data/store/TimerRegistration");

        let response = self
            .delete::<MilltimeRowResponse<serde_json::Value>>(url)
            .await?;

        match response.success {
            true => Ok(()),
            false => Err(MilltimeFetchError::Other(
                "milltime responded with success=false".to_string(),
            )),
        }
    }

    pub async fn save_timer(
        &self,
        save_timer_payload: domain::SaveTimerPayload,
    ) -> Result<domain::SaveTimerProjectRegistration, MilltimeFetchError> {
        let url = MilltimeURL::from_env().append_path("/data/store/TimerRegistration");

        let result = self
            .put::<MilltimeRowResponse<domain::SaveTimerResponse>>(url, Some(save_timer_payload))
            .await?;

        let registration = result.only_row()?.project_registration;
        Ok(registration)
    }

    pub async fn edit_timer(
        &self,
        edit_timer_payload: &domain::EditTimerPayload,
    ) -> Result<(), MilltimeFetchError> {
        let update_timer_filter = UpdateTimerFilter::new(edit_timer_payload.user_note.clone());
        let url = MilltimeURL::from_env()
            .append_path("/data/store/TimerRegistration")
            .with_filter(&update_timer_filter);

        let result = self
            .put::<MilltimeRowResponse<serde_json::Value>>(url, Some(edit_timer_payload))
            .await?;

        match result.success {
            true => Ok(()),
            false => Err(MilltimeFetchError::Other(
                "milltime responded with success=false".to_string(),
            )),
        }
    }

    pub async fn new_project_registration(
        &self,
        project_registration_payload: &domain::ProjectRegistrationPayload,
    ) -> Result<domain::ProjectRegistrationResponse, MilltimeFetchError> {
        let url = MilltimeURL::from_env().append_path("/data/store/ProjectRegistrationReact");

        let result = self
            .post::<MilltimeRowResponse<domain::ProjectRegistrationResponse>>(
                url,
                project_registration_payload,
            )
            .await?
            .only_row()?;

        Ok(result)
    }
}

trait ReqwestBuilderExt
where
    Self: Sized,
{
    fn milltime_headers(self, credentials: &Credentials) -> Self;
}

impl ReqwestBuilderExt for reqwest::RequestBuilder {
    fn milltime_headers(self, credentials: &Credentials) -> Self {
        self.header("Cookie", credentials.as_cookie_header())
            .header("X-Csrf-Token", credentials.csrf_token.clone())
            .header("User-Agent","Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
    }
}

#[derive(Error, Debug)]
pub enum MilltimeFetchError {
    #[error("Unauthorized")]
    Unauthorized,
    #[error("ResponseError: {0}")]
    ResponseError(String),
    #[error("ParsingError: {0}")]
    ParsingError(String),
    #[error("Other: {0}")]
    Other(String),
}

/// This is a generic response from Milltime. It contains a list of rows and a boolean indicating
/// whether the request was successful.
#[derive(Debug, Serialize, Deserialize)]
pub struct MilltimeRowResponse<T: Serialize> {
    pub rows: Vec<T>,
    pub success: bool,
}

impl<T: Serialize> MilltimeRowResponse<T> {
    pub fn only_row(self) -> Result<T, MilltimeFetchError> {
        if self.rows.len() == 1 {
            Ok(self.rows.into_iter().next().unwrap())
        } else {
            Err(MilltimeFetchError::ParsingError(format!(
                "Expected exactly one row, got {}. Response: {}",
                self.rows.len(),
                serde_json::to_string(&self).unwrap()
            )))
        }
    }
}
