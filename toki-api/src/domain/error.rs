use thiserror::Error;

/// Errors that can occur during time tracking operations.
#[derive(Debug, Clone, Error)]
pub enum TimeTrackingError {
    #[error("timer not found")]
    TimerNotFound,
    #[error("timer already running")]
    TimerAlreadyRunning,
    #[error("no timer running")]
    NoTimerRunning,
    #[allow(dead_code)]
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[allow(dead_code)]
    #[error("activity not found: {0}")]
    ActivityNotFound(String),
    #[error("invalid project/activity selection: {0}")]
    InvalidProjectActivity(String),
    #[error("the selected time period is locked")]
    LockedPeriod,
    #[error("idempotency key was already used with a different request")]
    IdempotencyConflict,
    #[error("an operation with this idempotency key is still in progress")]
    IdempotencyInProgress,
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Unknown(String),
}

impl TimeTrackingError {
    pub fn unknown(msg: impl Into<String>) -> Self {
        Self::Unknown(msg.into())
    }
}

/// Errors that can occur during avatar operations.
#[derive(Debug, Error)]
pub enum AvatarError {
    #[error("avatar not found")]
    NotFound,
    #[error("invalid image payload")]
    InvalidImage,
    #[error("avatar payload exceeds limit")]
    PayloadTooLarge,
    #[error("unsupported media type")]
    UnsupportedMediaType,
    #[error("avatar storage error: {0}")]
    Storage(String),
}

/// Errors that can occur during API token operations.
#[derive(Debug, Error)]
pub enum ApiTokenError {
    #[error("token name must contain 1 to 64 characters")]
    InvalidName,
    #[error("token limit reached")]
    TooManyTokens,
    #[error("token not found")]
    NotFound,
    #[error("api token storage error: {0}")]
    Storage(String),
}
