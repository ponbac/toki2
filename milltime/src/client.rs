use super::Credentials;
use crate::{
    domain::{
        self, ActivityFilter, DateFilter, ProjectSearchFilter, TimerRegistrationFilter,
        TimerRegistrationPayload,
    },
    milltime_url::MilltimeURL,
    UpdateTimerFilter,
};
use reqwest::header::{self, HeaderMap};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub struct MilltimeClient {
    credentials: Credentials,
    client: reqwest::Client,
    milltime_url: MilltimeURL,
}

impl MilltimeClient {
    pub fn new(credentials: Credentials) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .default_headers({
                let mut headers = HeaderMap::new();
                headers.insert(header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".parse().unwrap());
                headers.insert(header::COOKIE, credentials.as_cookie_header().parse().unwrap());
                headers.insert("X-Csrf-Token", credentials.csrf_token.parse().unwrap());
                headers
            })
            .build()
            .expect("Failed to create reqwest client");

        Self {
            credentials,
            client,
            milltime_url: MilltimeURL::from_env(),
        }
    }

    pub fn user_id(&self) -> &str {
        &self.credentials.user_id
    }

    async fn handle_response<T: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, MilltimeFetchError> {
        if resp.status().is_client_error() {
            return match resp.status() {
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                    Err(MilltimeFetchError::Unauthorized)
                }
                _ => Err(MilltimeFetchError::ResponseError(resp.status().to_string())),
            };
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

    async fn get<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let resp = self.client.get(url.as_ref()).send().await?;
        self.handle_response(resp).await
    }

    /// Helper method to get a single row from a Milltime response (the most common Milltime response format).
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
        let resp = self.client.post(url.as_ref()).json(&payload).send().await?;
        self.handle_response(resp).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
        payload: Option<impl serde::Serialize>,
    ) -> Result<T, MilltimeFetchError> {
        let mut client = self.client.put(url.as_ref());
        if let Some(payload) = payload {
            client = client.json(&payload);
        }

        let resp = client.send().await?;
        self.handle_response(resp).await
    }

    async fn delete<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let resp = self.client.delete(url.as_ref()).send().await?;
        self.handle_response(resp).await
    }

    pub async fn fetch_time_period_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimePeriodInfo, MilltimeFetchError> {
        let url = self
            .milltime_url
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
        let url = self
            .milltime_url
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
        let url = self
            .milltime_url
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
        let url = self
            .milltime_url
            .append_path("/data/store/ProjectPhaseActivity")
            .with_filter(&activity_filter);

        let root = self.get_single_row::<domain::ActivitiesRoot>(url).await?;

        Ok(root.activities)
    }

    pub async fn fetch_time_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimeInfo, MilltimeFetchError> {
        let url = self
            .milltime_url
            .append_path("/data/store/TimeInfo")
            .with_filter(&date_filter);

        let time_info = self.get_single_row::<domain::TimeInfo>(url).await?;

        Ok(time_info)
    }

    pub async fn fetch_timer(&self) -> Result<domain::TimerRegistration, MilltimeFetchError> {
        let url = self
            .milltime_url
            .append_path("/data/store/TimerRegistration");

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
        let url = self
            .milltime_url
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
        let url = self
            .milltime_url
            .append_path("/data/store/TimerRegistration");

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
        let url = self
            .milltime_url
            .append_path("/data/store/TimerRegistration");

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
        let url = self
            .milltime_url
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
        let url = self
            .milltime_url
            .append_path("/data/store/ProjectRegistrationReact");

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

impl From<reqwest::Error> for MilltimeFetchError {
    fn from(e: reqwest::Error) -> Self {
        Self::ResponseError(e.to_string())
    }
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
