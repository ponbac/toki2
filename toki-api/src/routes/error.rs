use axum::{
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::fmt;

use crate::{
    adapters::inbound::http::{ErrorResponse, TimeTrackingServiceError, WorkItemServiceError},
    app_state::AppStateError,
    domain::{
        AiProjectMappingError, AiSubscriptionError, AiUsageError, AvatarError, TimeTrackingError,
        WorkItemError,
    },
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
        let body = ErrorResponse {
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
            TimeTrackingError::InvalidInput(message) => Self::bad_request(message),
            TimeTrackingError::Conflict(message) => Self::conflict(message),
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

impl From<AiUsageError> for ApiError {
    fn from(err: AiUsageError) -> Self {
        match err {
            AiUsageError::InvalidUpload(message) | AiUsageError::InvalidQuery(message) => {
                Self::bad_request(message)
            }
            AiUsageError::MachineOwnedByAnotherUser => Self::forbidden(err.to_string()),
            AiUsageError::NumericRange => {
                Self::new(StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
            }
            AiUsageError::Storage(message) => {
                tracing::error!("AI usage operation failed: {}", message);
                Self::internal("ai usage operation failed")
            }
        }
    }
}

impl From<AiProjectMappingError> for ApiError {
    fn from(err: AiProjectMappingError) -> Self {
        match err {
            AiProjectMappingError::InvalidProjectKey(message) => Self::bad_request(message),
            AiProjectMappingError::Unattributed | AiProjectMappingError::UnknownProject(_) => {
                Self::bad_request(err.to_string())
            }
            AiProjectMappingError::NotInOwnUsage
            | AiProjectMappingError::AlreadyMapped
            | AiProjectMappingError::AdminOnly => Self::forbidden(err.to_string()),
            AiProjectMappingError::NotFound => Self::not_found(err.to_string()),
            // Reported once at startup; each request only says so.
            AiProjectMappingError::NotConfigured => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string())
            }
            AiProjectMappingError::ProjectsUnavailable(message) => {
                tracing::error!("Time-tracking projects are unavailable: {}", message);
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "time-tracking projects are unavailable",
                )
            }
            AiProjectMappingError::Storage(message) => {
                tracing::error!("AI project mapping operation failed: {}", message);
                Self::internal("ai project mapping operation failed")
            }
        }
    }
}

impl From<AiSubscriptionError> for ApiError {
    fn from(err: AiSubscriptionError) -> Self {
        match err {
            AiSubscriptionError::Invalid(message) => Self::bad_request(message),
            AiSubscriptionError::NotFound | AiSubscriptionError::UserNotFound => {
                Self::not_found(err.to_string())
            }
            AiSubscriptionError::Overlap(_) => Self::conflict(err.to_string()),
            AiSubscriptionError::Storage(message) => {
                tracing::error!("AI subscription operation failed: {}", message);
                Self::internal("ai subscription operation failed")
            }
        }
    }
}

/// A body that is not the expected JSON, including a missing or wrong content
/// type, is a bad request with the usual JSON error body. Use it through
/// `WithRejection<Json<T>, ApiError>`.
impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::bad_request(format!("invalid request body: {}", rejection.body_text()))
    }
}

/// A malformed path parameter, through `WithRejection<Path<T>, ApiError>`.
impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        Self::bad_request(format!("invalid path: {}", rejection.body_text()))
    }
}

/// A malformed query string, through `WithRejection<Query<T>, ApiError>`.
impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::bad_request(format!("invalid query: {}", rejection.body_text()))
    }
}
