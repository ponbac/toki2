use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;

use super::Credentials;

pub struct Client {
    credentials: Credentials,
}

impl Client {
    pub fn new(credentials: Credentials) -> Self {
        Self { credentials }
    }

    pub async fn fetch<T: DeserializeOwned>(
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
