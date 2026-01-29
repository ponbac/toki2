//! HTTP adapter for time tracking operations.
//!
//! Defines the factory trait for creating TimeTrackingService instances from HTTP requests.
//! The concrete implementation lives in `crate::factory` (the composition root).

use async_trait::async_trait;
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;

use crate::domain::ports::inbound::TimeTrackingService;

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

    #[allow(dead_code)]
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

/// Factory trait for creating TimeTrackingService instances from HTTP cookies.
///
/// This trait lives in the inbound adapter because it uses `CookieJar` (an HTTP type).
/// The concrete implementation lives in `crate::factory` where it's allowed to know
/// about concrete outbound adapters (MilltimeAdapter, PostgresTimerHistoryAdapter, etc.).
#[async_trait]
pub trait TimeTrackingServiceFactory: Send + Sync + 'static {
    /// Create a TimeTrackingService from cookie credentials.
    ///
    /// Always includes timer history support — the factory owns the timer repo.
    async fn create_service(
        &self,
        jar: CookieJar,
        cookie_domain: &str,
    ) -> Result<(Box<dyn TimeTrackingService>, CookieJar), TimeTrackingServiceError>;

    /// Validate credentials and return a CookieJar with auth cookies set.
    async fn authenticate(
        &self,
        username: &str,
        password: &str,
        cookie_domain: &str,
    ) -> Result<CookieJar, TimeTrackingServiceError>;
}
