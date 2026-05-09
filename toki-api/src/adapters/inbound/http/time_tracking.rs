//! HTTP adapter for time tracking operations.
//!
//! Defines the factory trait for creating TimeTrackingService instances from HTTP requests.
//! The concrete implementation lives in `crate::factory` (the composition root).

use async_trait::async_trait;
use axum::http::StatusCode;

use crate::domain::{models::UserId, ports::inbound::TimeTrackingService};

/// Error returned when creating or validating a TimeTrackingService fails.
#[derive(Debug)]
pub struct TimeTrackingServiceError {
    pub status: StatusCode,
    pub message: String,
}

impl TimeTrackingServiceError {
    pub fn configuration(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    pub fn not_connected(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
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

/// Factory trait for creating TimeTrackingService instances from authenticated Toki users.
///
/// The concrete implementation lives in `crate::factory` where it's allowed to know about
/// concrete outbound adapters (KleerAdapter, PostgresTimerHistoryAdapter, etc.).
#[async_trait]
pub trait TimeTrackingServiceFactory: Send + Sync + 'static {
    /// Create a TimeTrackingService for an authenticated local Toki user.
    ///
    /// Always includes timer history support — the factory owns the timer repo.
    async fn create_service(
        &self,
        user_id: UserId,
    ) -> Result<Box<dyn TimeTrackingService>, TimeTrackingServiceError>;
}
