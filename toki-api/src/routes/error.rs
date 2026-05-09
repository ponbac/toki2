use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::fmt;

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

use crate::{
    adapters::inbound::http::{TimeTrackingServiceError, WorkItemServiceError},
    app_state::AppStateError,
    domain::{AvatarError, TimeTrackingError, WorkItemError},
    repositories::RepositoryError,
};

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
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
        let body = ErrorBody {
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
            TimeTrackingError::TimerNotFound
            | TimeTrackingError::NoTimerRunning
            | TimeTrackingError::ProjectNotFound(_)
            | TimeTrackingError::ActivityNotFound(_) => Self::not_found(err.to_string()),
            TimeTrackingError::TimerAlreadyRunning => Self::conflict(err.to_string()),
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
