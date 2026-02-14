use thiserror::Error;

/// Errors that can occur during time tracking operations.
#[derive(Debug, Error)]
pub enum TimeTrackingError {
    #[error("timer not found")]
    TimerNotFound,
    #[error("timer already running")]
    TimerAlreadyRunning,
    #[error("no timer running")]
    NoTimerRunning,
    #[error("authentication failed")]
    AuthenticationFailed,
    #[allow(dead_code)]
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[allow(dead_code)]
    #[error("activity not found: {0}")]
    ActivityNotFound(String),
    #[error("invalid date range")]
    InvalidDateRange,
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
