use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;

use crate::{
    domain::{self, DateFilter, TimerRegistrationFilter, TimerRegistrationPayload},
    milltime_url::MilltimeURL,
};

use super::Credentials;

pub struct MilltimeClient {
    credentials: Credentials,
}

impl MilltimeClient {
    pub fn new(credentials: Credentials) -> Self {
        Self { credentials }
    }

    async fn fetch<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let client = reqwest::Client::new();

        let resp = client
            .get(url.as_ref())
            .header("Cookie", self.credentials.as_cookie_header())
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

    async fn fetch_single_row<T: DeserializeOwned>(
        &self,
        url: impl AsRef<str>,
    ) -> Result<T, MilltimeFetchError> {
        let response: MilltimeRowResponse<T> = self.fetch(url).await?;
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
            .header("Cookie", self.credentials.as_cookie_header())
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

    pub async fn fetch_time_period_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimePeriodInfo, MilltimeFetchError> {
        let url = MilltimeURL::new()
            .append_path("/data/store/TimeInfo")
            .with_filter(&date_filter);

        let time_period_info = self.fetch_single_row::<domain::TimePeriodInfo>(url).await?;

        Ok(domain::TimePeriodInfo {
            from: date_filter.from,
            to: date_filter.to,
            ..time_period_info
        })
    }

    pub async fn fetch_user_calendar(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::UserCalendar, MilltimeFetchError> {
        let url = MilltimeURL::new()
            .append_path("/data/store/UserCalendar")
            .with_filter(&date_filter);

        let raw_calendar = self
            .fetch_single_row::<domain::RawUserCalendar>(url)
            .await?;

        let transformed_weeks = raw_calendar
            .weeks
            .into_iter()
            .map(domain::Week::from)
            .collect();

        Ok(domain::UserCalendar {
            weeks: transformed_weeks,
        })
    }

    pub async fn start_timer(
        &self,
        start_timer_options: domain::StartTimerOptions,
    ) -> Result<(), MilltimeFetchError> {
        let payload: TimerRegistrationPayload = start_timer_options.into();
        let reg_timer_url_filter: TimerRegistrationFilter = (&payload).into();
        let url = MilltimeURL::new()
            .append_path("/data/store/TimerRegistration")
            .with_filter(&reg_timer_url_filter);

        let _response = self.post::<serde_json::Value>(url, payload).await?;

        Ok(())
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
#[derive(Debug, Deserialize)]
pub struct MilltimeRowResponse<T> {
    pub rows: Vec<T>,
    pub success: bool,
}

impl<T> MilltimeRowResponse<T> {
    pub fn only_row(self) -> Result<T, MilltimeFetchError> {
        if self.rows.len() == 1 {
            Ok(self.rows.into_iter().next().unwrap())
        } else {
            Err(MilltimeFetchError::ParsingError(format!(
                "Expected exactly one row, got {}",
                self.rows.len()
            )))
        }
    }
}
