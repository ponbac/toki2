use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;

use crate::{
    domain::{self, RawUserCalendar, TimePeriodInfo},
    MilltimeURL,
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

    pub async fn fetch_time_period_info(
        &self,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<TimePeriodInfo, MilltimeFetchError> {
        let url = crate::MilltimeURL::new()
            .append_path("/data/store/TimeInfo")
            .with_date_filter(&from, &to);

        let response: MilltimeRowResponse<domain::TimePeriodInfo> = self.fetch(url).await?;
        let result_row = response.only_row()?;

        Ok(domain::TimePeriodInfo {
            from: Some(from),
            to: Some(to),
            ..result_row
        })
    }

    pub async fn fetch_user_calendar(
        &self,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<domain::UserCalendar, MilltimeFetchError> {
        let url = MilltimeURL::new()
            .append_path("/data/store/UserCalendar")
            .with_date_filter(&from, &to);

        let response: MilltimeRowResponse<domain::RawUserCalendar> = self.fetch(url).await?;
        let raw_result = response.only_row()?;

        let transformed_weeks = raw_result
            .weeks
            .into_iter()
            .map(domain::Week::from)
            .collect();

        Ok(domain::UserCalendar {
            weeks: transformed_weeks,
        })
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
