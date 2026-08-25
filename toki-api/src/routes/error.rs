use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::fmt;

use crate::{
    adapters::inbound::http::{
        ErrorResponse, TimeTrackingServiceError, TimeTrackingServiceErrorKind, WorkItemServiceError,
    },
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

    fn service_unavailable(code: &'static str, message: &'static str) -> Self {
        Self::coded(StatusCode::SERVICE_UNAVAILABLE, code, message)
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
            TimeTrackingError::ProviderUnavailable(message) => {
                tracing::error!(error = %message, "Time tracking provider unavailable");
                Self::service_unavailable(
                    "time_tracking_provider_unavailable",
                    "time tracking provider is unavailable",
                )
            }
            TimeTrackingError::StorageUnavailable(message) => {
                tracing::error!(error = %message, "Time tracking storage unavailable");
                Self::service_unavailable(
                    "time_tracking_storage_unavailable",
                    "time tracking storage is unavailable",
                )
            }
            _ => Self::internal(err.to_string()),
        }
    }
}

impl From<TimeTrackingServiceError> for ApiError {
    fn from(err: TimeTrackingServiceError) -> Self {
        match err.kind {
            TimeTrackingServiceErrorKind::Provider => {
                tracing::error!(error = %err.message, "Time tracking provider unavailable");
                Self::service_unavailable(
                    "time_tracking_provider_unavailable",
                    "time tracking provider is unavailable",
                )
            }
            TimeTrackingServiceErrorKind::Storage => {
                tracing::error!(error = %err.message, "Time tracking storage unavailable");
                Self::service_unavailable(
                    "time_tracking_storage_unavailable",
                    "time tracking storage is unavailable",
                )
            }
            TimeTrackingServiceErrorKind::Other => Self::new(err.status, err.message),
        }
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
                Self::service_unavailable(
                    "work_item_provider_unavailable",
                    "work item provider is unavailable",
                )
            }
        }
    }
}

impl From<WorkItemServiceError> for ApiError {
    fn from(err: WorkItemServiceError) -> Self {
        if err.status == StatusCode::SERVICE_UNAVAILABLE {
            tracing::error!(error = %err.message, "Work item provider unavailable");
            Self::service_unavailable(
                "work_item_provider_unavailable",
                "work item provider is unavailable",
            )
        } else {
            Self::new(err.status, err.message)
        }
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

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, response::IntoResponse};

    use super::*;

    async fn response_parts(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice(&body).unwrap();
        (status, body)
    }

    #[tokio::test]
    async fn time_tracking_provider_failures_are_safe_service_unavailable_responses() {
        let (status, body) = response_parts(
            TimeTrackingError::provider_unavailable("secret provider response").into(),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "time_tracking_provider_unavailable");
        assert_eq!(body["error"], "time tracking provider is unavailable");
    }

    #[tokio::test]
    async fn time_tracking_storage_failures_are_distinct_safe_responses() {
        let (status, body) = response_parts(
            TimeTrackingError::storage_unavailable("postgres connection string").into(),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "time_tracking_storage_unavailable");
        assert_eq!(body["error"], "time tracking storage is unavailable");
    }

    #[tokio::test]
    async fn work_item_provider_failures_are_safe_service_unavailable_responses() {
        let (status, body) = response_parts(
            WorkItemError::ProviderError("secret provider response".to_string()).into(),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "work_item_provider_unavailable");
        assert_eq!(body["error"], "work item provider is unavailable");
    }

    #[tokio::test]
    async fn factory_configuration_failures_do_not_expose_details() {
        let (status, body) = response_parts(
            TimeTrackingServiceError::configuration("secret configuration detail").into(),
        )
        .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "time_tracking_provider_unavailable");
        assert_eq!(body["error"], "time tracking provider is unavailable");
    }

    #[tokio::test]
    async fn factory_storage_failures_keep_the_storage_contract() {
        let (status, body) =
            response_parts(TimeTrackingServiceError::storage("secret database detail").into())
                .await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "time_tracking_storage_unavailable");
        assert_eq!(body["error"], "time tracking storage is unavailable");
    }

    #[tokio::test]
    async fn domain_failures_keep_existing_client_statuses() {
        let cases = [
            (
                TimeTrackingError::InvalidInput("invalid".into()),
                StatusCode::BAD_REQUEST,
            ),
            (
                TimeTrackingError::Forbidden("denied".into()),
                StatusCode::FORBIDDEN,
            ),
            (TimeTrackingError::TimerNotFound, StatusCode::NOT_FOUND),
            (TimeTrackingError::LockedPeriod, StatusCode::CONFLICT),
        ];

        for (error, expected) in cases {
            assert_eq!(response_parts(error.into()).await.0, expected);
        }
    }
}
