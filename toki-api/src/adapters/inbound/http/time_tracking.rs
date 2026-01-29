//! HTTP adapter for time tracking operations.
//!
//! Provides helper traits for creating TimeTrackingService instances from HTTP requests.

use std::sync::Arc;

use axum::http::StatusCode;
use axum_extra::extract::CookieJar;

use crate::{
    adapters::outbound::{milltime::MilltimeAdapter, postgres::PostgresTimerHistoryAdapter},
    domain::{
        ports::inbound::TimeTrackingService,
        services::TimeTrackingServiceImpl,
        MilltimePassword,
    },
    repositories::TimerRepositoryImpl,
};

/// Error returned when creating a TimeTrackingService from cookies fails.
#[derive(Debug)]
pub struct TimeTrackingServiceError {
    pub status: StatusCode,
    pub message: String,
}

impl TimeTrackingServiceError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

/// Extension trait for creating a TimeTrackingService from a CookieJar.
///
/// This provides the bridge between HTTP requests (with authentication cookies)
/// and the domain service layer.
pub trait TimeTrackingServiceExt: Sized {
    /// Create a TimeTrackingService from the authentication cookies.
    ///
    /// Returns the service and an updated CookieJar (which may contain refreshed tokens).
    async fn into_time_tracking_service(
        self,
        cookie_domain: &str,
    ) -> Result<(impl TimeTrackingService, Self), TimeTrackingServiceError>;

    /// Create a TimeTrackingService with timer history repository support.
    ///
    /// Use this for operations that need to merge with local timer history
    /// (like get_time_entries, create_time_entry, edit_time_entry).
    async fn into_time_tracking_service_with_history(
        self,
        cookie_domain: &str,
        timer_repo: Arc<TimerRepositoryImpl>,
    ) -> Result<(impl TimeTrackingService, Self), TimeTrackingServiceError>;
}

impl TimeTrackingServiceExt for CookieJar {
    async fn into_time_tracking_service(
        self,
        cookie_domain: &str,
    ) -> Result<(impl TimeTrackingService, Self), TimeTrackingServiceError> {
        let (credentials, jar) = extract_credentials(self, cookie_domain).await?;

        let adapter = MilltimeAdapter::new(credentials);
        let service = TimeTrackingServiceImpl::new(Arc::new(adapter));

        Ok((service, jar))
    }

    async fn into_time_tracking_service_with_history(
        self,
        cookie_domain: &str,
        timer_repo: Arc<TimerRepositoryImpl>,
    ) -> Result<(impl TimeTrackingService, Self), TimeTrackingServiceError> {
        let (credentials, jar) = extract_credentials(self, cookie_domain).await?;

        let adapter = MilltimeAdapter::new(credentials);
        let history_adapter = PostgresTimerHistoryAdapter::new(timer_repo);
        let service =
            TimeTrackingServiceImpl::with_timer_repo(Arc::new(adapter), Arc::new(history_adapter));

        Ok((service, jar))
    }
}

/// Extract Milltime credentials from cookies.
///
/// First tries to parse existing credential cookies, then falls back to
/// username/password authentication.
async fn extract_credentials(
    jar: CookieJar,
    cookie_domain: &str,
) -> Result<(milltime::Credentials, CookieJar), TimeTrackingServiceError> {
    // Try to use existing credentials from cookies first
    if let Ok(credentials) = jar.clone().try_into() {
        tracing::debug!("using existing milltime credentials");
        return Ok((credentials, jar));
    }

    // Fall back to username/password authentication
    let user_cookie = jar.get("mt_user").ok_or_else(|| {
        TimeTrackingServiceError::unauthorized("missing mt_user cookie")
    })?;

    let pass_cookie = jar.get("mt_password").ok_or_else(|| {
        TimeTrackingServiceError::unauthorized("missing mt_password cookie")
    })?;

    let decrypted_pass = MilltimePassword::from_encrypted(pass_cookie.value().to_string());

    let credentials = milltime::Credentials::new(user_cookie.value(), decrypted_pass.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("failed to create milltime credentials: {:?}", e);
            TimeTrackingServiceError::unauthorized(e.to_string())
        })?;

    // Update cookies with the new credentials
    let mut updated_jar = jar;
    for cookie in credentials.auth_cookies(cookie_domain.to_string()) {
        updated_jar = updated_jar.add(cookie);
    }

    tracing::debug!("created new milltime credentials");
    Ok((credentials, updated_jar))
}
