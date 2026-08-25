use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::fmt;

use crate::{
    adapters::inbound::http::{ErrorResponse, TimeTrackingServiceError, WorkItemServiceError},
    app_state::AppStateError,
    domain::{AvatarError, TimeTrackingError, WorkItemError},
    repositories::RepositoryError,
};

pub struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        let code = match status {
            StatusCode::BAD_REQUEST => "bad_request",
            StatusCode::UNAUTHORIZED => "unauthorized",
            StatusCode::FORBIDDEN => "forbidden",
            StatusCode::NOT_FOUND => "not_found",
            StatusCode::CONFLICT => "conflict",
            StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
            _ => "internal_error",
        };
        Self::coded(status, code, message)
    }

    pub fn coded(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.status, self.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            code: self.code,
            error: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<RepositoryError> for ApiError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::DatabaseError(ref e) => {
                tracing::error!("Database error: {:?}", e);
                Self::internal(err.to_string())
            }
            RepositoryError::NotFound(_) => Self::not_found(err.to_string()),
        }
    }
}

impl From<AppStateError> for ApiError {
    fn from(err: AppStateError) -> Self {
        match &err {
            AppStateError::RepoClientNotFound(_) => Self::not_found(err.to_string()),
            AppStateError::WebPushError(e) => {
                tracing::error!("Web push error: {:?}", e);
                Self::internal(err.to_string())
            }
        }
    }
}

impl From<TimeTrackingError> for ApiError {
    fn from(err: TimeTrackingError) -> Self {
        match err {
            TimeTrackingError::TimerNotFound => Self::coded(
                StatusCode::NOT_FOUND,
                "time_entry_not_found",
                err.to_string(),
            ),
            TimeTrackingError::NoTimerRunning => {
                Self::coded(StatusCode::NOT_FOUND, "no_active_timer", err.to_string())
            }
            TimeTrackingError::ProjectNotFound(_) | TimeTrackingError::ActivityNotFound(_) => {
                Self::not_found(err.to_string())
            }
            TimeTrackingError::TimerAlreadyRunning => Self::coded(
                StatusCode::CONFLICT,
                "timer_already_running",
                err.to_string(),
            ),
            TimeTrackingError::InvalidProjectActivity(message) => {
                Self::coded(StatusCode::BAD_REQUEST, "invalid_project_activity", message)
            }
            TimeTrackingError::LockedPeriod => {
                Self::coded(StatusCode::CONFLICT, "locked_period", err.to_string())
            }
            TimeTrackingError::IdempotencyConflict => Self::coded(
                StatusCode::CONFLICT,
                "idempotency_conflict",
                err.to_string(),
            ),
            TimeTrackingError::IdempotencyInProgress => Self::coded(
                StatusCode::CONFLICT,
                "idempotency_in_progress",
                err.to_string(),
            ),
            TimeTrackingError::InvalidInput(message) => Self::bad_request(message),
            TimeTrackingError::Forbidden(message) => Self::forbidden(message),
            _ => Self::internal(err.to_string()),
        }
    }
}

impl From<TimeTrackingServiceError> for ApiError {
    fn from(err: TimeTrackingServiceError) -> Self {
        Self::new(err.status, err.message)
    }
}

impl From<AvatarError> for ApiError {
    fn from(err: AvatarError) -> Self {
        match err {
            AvatarError::NotFound => Self::not_found("avatar not found"),
            AvatarError::InvalidImage => Self::bad_request("invalid image payload"),
            AvatarError::PayloadTooLarge => Self::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "avatar payload exceeds limit",
            ),
            AvatarError::UnsupportedMediaType => {
                Self::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported media type")
            }
            AvatarError::Storage(message) => {
                tracing::error!("Avatar operation failed: {}", message);
                Self::internal("avatar operation failed")
            }
        }
    }
}

impl From<WorkItemError> for ApiError {
    fn from(err: WorkItemError) -> Self {
        match err {
            WorkItemError::InvalidInput(message) => Self::bad_request(message),
            WorkItemError::ProviderError(message) => {
                tracing::error!("Work item provider operation failed: {}", message);
                Self::internal("work item provider operation failed")
            }
        }
    }
}

impl From<WorkItemServiceError> for ApiError {
    fn from(err: WorkItemServiceError) -> Self {
        Self::new(err.status, err.message)
    }
}

impl From<crate::domain::ApiTokenError> for ApiError {
    fn from(err: crate::domain::ApiTokenError) -> Self {
        match err {
            crate::domain::ApiTokenError::InvalidName => Self::bad_request(err.to_string()),
            crate::domain::ApiTokenError::TooManyTokens => Self::conflict(err.to_string()),
            crate::domain::ApiTokenError::NotFound => Self::not_found(err.to_string()),
            crate::domain::ApiTokenError::Storage(message) => {
                tracing::error!("API token operation failed: {}", message);
                Self::internal("api token operation failed")
            }
        }
    }
}
