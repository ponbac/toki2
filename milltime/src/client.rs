use std::str::FromStr;

use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;

use crate::{domain, milltime_url::MilltimeURL};

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

    pub async fn fetch_time_period_info(
        &self,
        date_filter: DateFilter,
    ) -> Result<domain::TimePeriodInfo, MilltimeFetchError> {
        let url = MilltimeURL::new()
            .append_path("/data/store/TimeInfo")
            .with_date_filter(&date_filter);

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
            .with_date_filter(&date_filter);

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

pub struct DateFilter {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
}

impl DateFilter {
    pub fn new(from: chrono::NaiveDate, to: chrono::NaiveDate) -> Self {
        Self { from, to }
    }

    pub fn as_milltime_filter(&self) -> String {
        format!(
            "[[\"fromDate\",\"=\",\"{}\"],[\"toDate\",\"=\",\"{}\"]]",
            self.from, self.to
        )
    }
}

impl FromStr for DateFilter {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        let from = chrono::NaiveDate::parse_from_str(parts[0], "%Y-%m-%d")?;
        let to = chrono::NaiveDate::parse_from_str(parts[1], "%Y-%m-%d")?;

        Ok(Self { from, to })
    }
}
