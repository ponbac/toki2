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
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Conflict(String),
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

/// Errors that can occur while storing or reading AI usage.
#[derive(Debug, Error)]
pub enum AiUsageError {
    #[error("{0}")]
    InvalidUpload(String),
    #[error("machine is registered to another user")]
    MachineOwnedByAnotherUser,
    /// An aggregate cannot be represented by the report's finite costs and
    /// JavaScript-safe integer counts. Stored usage is retained unchanged.
    #[error("ai usage result exceeds the supported numeric range")]
    NumericRange,
    /// A read of usage asked for something it cannot answer, such as a range
    /// that ends before it starts.
    #[error("{0}")]
    InvalidQuery(String),
    #[error("ai usage storage error: {0}")]
    Storage(String),
}

/// Errors that can occur while managing AI usage project mappings.
#[derive(Debug, Error)]
pub enum AiProjectMappingError {
    #[error("{0}")]
    InvalidProjectKey(String),
    #[error("unattributed usage cannot be mapped to a project")]
    Unattributed,
    #[error("the project key does not appear in your AI usage")]
    NotInOwnUsage,
    #[error("the project key is already mapped; ask an admin to change its mapping")]
    AlreadyMapped,
    #[error("only an admin signed in to Toki, not using an API token, can delete a mapping; ask an admin")]
    AdminOnly,
    #[error("time-tracking project {0} does not exist or is not active")]
    UnknownProject(String),
    #[error("no mapping exists for the project key")]
    NotFound,
    #[error("time tracking is not configured")]
    NotConfigured,
    #[error("time-tracking projects are unavailable: {0}")]
    ProjectsUnavailable(String),
    #[error("ai project mapping storage error: {0}")]
    Storage(String),
}

/// Errors that can occur while declaring or reading AI subscriptions.
#[derive(Debug, Error)]
pub enum AiSubscriptionError {
    #[error("{0}")]
    Invalid(String),
    #[error("subscription not found")]
    NotFound,
    #[error("user not found")]
    UserNotFound,
    #[error("the period overlaps another {} subscription of the same user", .0.as_str())]
    Overlap(crate::domain::models::AiProvider),
    #[error("ai subscription storage error: {0}")]
    Storage(String),
}

/// Errors that can occur while billing AI usage.
#[derive(Debug, Error)]
pub enum AiBillingError {
    #[error("user not found")]
    UserNotFound,
    #[error("ai billing totals exceed the supported numeric range")]
    NumericRange,
    #[error("{0}")]
    Invalid(String),
    #[error("ai billing storage error: {0}")]
    Storage(String),
}

/// Errors from an exchange-rate provider. Billing treats them as a missing
/// rate: it never guesses one.
#[derive(Debug, Clone, Error)]
pub enum ExchangeRateError {
    /// The provider refused because of its rate limit, possibly saying for how
    /// long.
    #[error("the exchange rate provider is rate limiting requests")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    /// The provider cannot be reached or failed: a timeout, a connection
    /// error, a server error, or an endpoint that does not exist.
    #[error("the exchange rate provider is unavailable: {0}")]
    Unavailable(String),
    /// A response for this rate that could not be read.
    #[error("unexpected exchange rate response: {0}")]
    Response(String),
}

impl ExchangeRateError {
    /// Whether the error concerns the provider as a whole, so no other rate
    /// should be asked for until it has had time to recover.
    pub fn is_provider_wide(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Unavailable(_))
    }

    /// How long the provider asked to wait, if it did.
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }
}
