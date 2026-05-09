use reqwest::{Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use time::Date;

use crate::types::{
    KleerActivityList, KleerClientProjectList, KleerEventList, KleerEventReadable,
    KleerEventRestrictionList, KleerEventWritable, KleerPayrollEventList, KleerSavedId,
    KleerScheduleMetadataList, KleerUserList, KleerUserMe,
};

pub const DEFAULT_BASE_URL: &str = "https://api.kleer.se/v1";
const JSON_CONTENT_TYPE: &str = "application/json";

#[derive(Clone, PartialEq, Eq)]
pub struct KleerCredentials {
    pub token: String,
    pub company_id: String,
    pub base_url: String,
}

impl fmt::Debug for KleerCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KleerCredentials")
            .field("token", &"[redacted]")
            .field("company_id", &self.company_id)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl KleerCredentials {
    pub fn new(
        token: impl Into<String>,
        company_id: impl Into<String>,
        base_url: Option<impl Into<String>>,
    ) -> Self {
        Self {
            token: token.into(),
            company_id: company_id.into(),
            base_url: normalize_base_url(base_url.map(Into::into).as_deref()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KleerError {
    #[error("invalid Kleer configuration: {0}")]
    InvalidConfig(String),
    #[error("Kleer authentication failed")]
    Unauthorized,
    #[error("Kleer access forbidden")]
    Forbidden,
    #[error("Kleer resource not found")]
    NotFound,
    #[error("Kleer request failed: {0}")]
    Request(String),
    #[error("Kleer returned {status}: {body}")]
    Response { status: StatusCode, body: String },
    #[error("failed to deserialize Kleer response: {message}; body: {body}")]
    Deserialize { message: String, body: String },
}

#[derive(Debug, Clone)]
pub struct KleerClient {
    http: Client,
    credentials: KleerCredentials,
}

impl KleerClient {
    pub fn new(credentials: KleerCredentials) -> Result<Self, KleerError> {
        if credentials.token.trim().is_empty() {
            return Err(KleerError::InvalidConfig("missing token".to_string()));
        }
        if credentials.company_id.trim().is_empty() {
            return Err(KleerError::InvalidConfig("missing company id".to_string()));
        }

        Ok(Self {
            http: Client::builder()
                .build()
                .map_err(|e| KleerError::Request(e.to_string()))?,
            credentials,
        })
    }

    pub fn credentials(&self) -> &KleerCredentials {
        &self.credentials
    }

    pub async fn validate_credentials(&self) -> Result<KleerUserMe, KleerError> {
        self.user_me().await
    }

    pub async fn user_me(&self) -> Result<KleerUserMe, KleerError> {
        self.get("user/me", &[]).await
    }

    pub async fn list_users(&self) -> Result<KleerUserList, KleerError> {
        self.get("user", &[]).await
    }

    pub async fn list_client_projects(&self) -> Result<KleerClientProjectList, KleerError> {
        self.get("client-project", &[]).await
    }

    pub async fn list_active_client_projects(&self) -> Result<KleerClientProjectList, KleerError> {
        self.get("client-project", &[("filter", "active".to_string())])
            .await
    }

    pub async fn list_activities(&self) -> Result<KleerActivityList, KleerError> {
        self.get("activity", &[]).await
    }

    pub async fn list_events(
        &self,
        user_id: i64,
        start_date: Date,
        end_date: Date,
    ) -> Result<KleerEventList, KleerError> {
        self.get(
            "event",
            &[
                ("userId", user_id.to_string()),
                ("startDate", start_date.to_string()),
                ("endDate", end_date.to_string()),
            ],
        )
        .await
    }

    pub async fn get_event(&self, event_id: i64) -> Result<KleerEventReadable, KleerError> {
        self.get(&format!("event/{event_id}"), &[]).await
    }

    pub async fn list_event_statuses(
        &self,
        user_id: i64,
        from_date: Date,
        to_date: Date,
    ) -> Result<KleerEventRestrictionList, KleerError> {
        self.get(
            "event/statuses",
            &[
                ("userId", user_id.to_string()),
                ("fromDate", from_date.to_string()),
                ("toDate", to_date.to_string()),
            ],
        )
        .await
    }

    pub async fn create_event(
        &self,
        body: &KleerEventWritable,
    ) -> Result<KleerSavedId, KleerError> {
        self.send_json(Method::PUT, "event", Some(body)).await
    }

    pub async fn update_event(
        &self,
        event_id: i64,
        body: &KleerEventWritable,
    ) -> Result<KleerSavedId, KleerError> {
        self.send_json(Method::POST, &format!("event/{event_id}"), Some(body))
            .await
    }

    pub async fn delete_event(&self, event_id: i64) -> Result<KleerSavedId, KleerError> {
        self.send_json::<(), KleerSavedId>(Method::DELETE, &format!("event/{event_id}"), None)
            .await
    }

    pub async fn list_schedule_summary(
        &self,
        user_id: i64,
        start_date: Date,
        end_date: Date,
    ) -> Result<KleerScheduleMetadataList, KleerError> {
        self.get(
            &format!("payroll/user/{user_id}/schedule/{start_date}/to/{end_date}"),
            &[],
        )
        .await
    }

    pub async fn list_payroll_events(
        &self,
        user_id: i64,
        from_date: Date,
        to_date: Date,
    ) -> Result<KleerPayrollEventList, KleerError> {
        self.get(
            &format!("payroll/user/{user_id}/event/from/{from_date}/to/{to_date}"),
            &[],
        )
        .await
    }

    async fn get<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T, KleerError>
    where
        T: DeserializeOwned,
    {
        let request = self.request(Method::GET, path).query(query);

        self.send(request).await
    }

    async fn send_json<B, T>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, KleerError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let request = self.request(method, path);

        let request = if let Some(body) = body {
            request.json(body)
        } else {
            request
        };

        self.send(request).await
    }

    async fn send<T>(&self, request: reqwest::RequestBuilder) -> Result<T, KleerError>
    where
        T: DeserializeOwned,
    {
        let response = request
            .send()
            .await
            .map_err(|e| KleerError::Request(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| KleerError::Request(e.to_string()))?;

        if !status.is_success() {
            return Err(match status {
                StatusCode::UNAUTHORIZED => KleerError::Unauthorized,
                StatusCode::FORBIDDEN => KleerError::Forbidden,
                StatusCode::NOT_FOUND => KleerError::NotFound,
                _ => KleerError::Response { status, body },
            });
        }

        serde_json::from_str(&body).map_err(|e| KleerError::Deserialize {
            message: e.to_string(),
            body,
        })
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, self.endpoint(path))
            .header("X-Token", &self.credentials.token)
            .header("Accept", JSON_CONTENT_TYPE)
            // Kleer uses Content-Type to select JSON responses even for some GET endpoints.
            .header("Content-Type", JSON_CONTENT_TYPE)
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/company/{}/{}",
            self.credentials.base_url, self.credentials.company_id, path
        )
    }
}

fn normalize_base_url(base_url: Option<&str>) -> String {
    let raw = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);

    raw.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_default_and_custom_base_urls() {
        assert_eq!(normalize_base_url(None), DEFAULT_BASE_URL);
        assert_eq!(
            normalize_base_url(Some("https://test-api.kleer.se/v1/")),
            "https://test-api.kleer.se/v1"
        );
    }

    #[test]
    fn rejects_missing_credentials() {
        let error = KleerClient::new(KleerCredentials::new("", "1", None::<String>)).unwrap_err();
        assert!(matches!(error, KleerError::InvalidConfig(_)));
    }

    #[test]
    fn sets_json_headers_on_get_requests() {
        let client = KleerClient::new(KleerCredentials::new("token", "4875", None::<String>))
            .expect("valid client");
        let request = client
            .request(Method::GET, "user/me")
            .build()
            .expect("request to build");

        assert_eq!(request.headers()["accept"], JSON_CONTENT_TYPE);
        assert_eq!(request.headers()["content-type"], JSON_CONTENT_TYPE);
    }
}
