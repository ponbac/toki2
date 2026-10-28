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

/// Errors of the OAuth authorization server. The variants that name an OAuth
/// error code map to it at the protocol boundary.
#[derive(Debug, Error)]
pub enum OAuthError {
    /// A malformed request: `invalid_request`.
    #[error("{0}")]
    InvalidRequest(String),
    /// An unknown or unacceptable client: `invalid_client`.
    #[error("{0}")]
    InvalidClient(String),
    /// An invalid, expired, revoked or replayed code or refresh token:
    /// `invalid_grant`.
    #[error("{0}")]
    InvalidGrant(String),
    /// A registration with an unacceptable redirect URI:
    /// `invalid_redirect_uri`.
    #[error("{0}")]
    InvalidRedirectUri(String),
    /// A registration with other unacceptable metadata:
    /// `invalid_client_metadata`.
    #[error("{0}")]
    InvalidClientMetadata(String),
    /// The client's metadata document could not be fetched.
    #[error("{0}")]
    ClientMetadataUnavailable(String),
    #[error("not found")]
    NotFound,
    #[error("oauth storage error: {0}")]
    Storage(String),
}

/// Why an authorization request cannot proceed.
#[derive(Debug)]
pub enum AuthorizationError {
    /// The client or its redirect URI cannot be trusted, so the error is shown
    /// to the user and never sent to the redirect URI.
    Unredirectable(OAuthError),
    /// The error goes back to the client at its redirect URI.
    Redirect(crate::domain::models::AuthorizationResponse),
}

/// Errors that can occur while storing or reading AI usage.
#[derive(Debug, Error)]
pub enum AiUsageError {
    #[error("{0}")]
    InvalidUpload(String),
    #[error("machine is registered to another user")]
    MachineOwnedByAnotherUser,
    /// The machine already stored an upload that read its history later.
    /// Nothing was changed.
    #[error("a newer upload from this machine is already stored")]
    SupersededUpload,
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
    #[error("the project key is not marked as personal")]
    NotPersonal,
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

/// Errors from a payer's account directory, such as GitHub's list of an
/// organization's Copilot seats. Modeled on `ExchangeRateError`; messages never
/// carry credentials.
#[derive(Debug, Clone, Error)]
pub enum ProviderAccountDirectoryError {
    /// The directory rejected Toki's credentials.
    #[error("the account directory rejected Toki's credentials: {0}")]
    Unauthorized(String),
    /// The directory refused, for example for a missing permission.
    #[error("the account directory refused: {0}")]
    Forbidden(String),
    /// The organization or the installation does not exist.
    #[error("the account directory has no such organization or installation: {0}")]
    NotFound(String),
    /// The directory refused because of its rate limit, possibly saying for
    /// how long.
    #[error("the account directory is rate limiting requests")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    /// A timeout, a connection error or a server error.
    #[error("the account directory is unavailable: {0}")]
    Unavailable(String),
    /// A response that could not be read.
    #[error("unexpected account directory response: {0}")]
    Response(String),
}

/// Errors of provider accounts, their seats and their dispositions.
#[derive(Debug, Error)]
pub enum AiProviderAccountError {
    #[error("{0}")]
    Invalid(String),
    #[error("disposition not found")]
    NotFound,
    #[error("account not found")]
    AccountNotFound,
    #[error("user not found")]
    UserNotFound,
    /// The dates share a day with another disposition of the account.
    #[error("the dates overlap another disposition of {login}")]
    AccountOverlap { login: String },
    /// The user is linked to another account (`login`) on some of the days.
    #[error("the user is already linked to {login} on some of these days")]
    UserOverlap { login: String },
    #[error("no account directory is configured")]
    DirectoryOff,
    #[error("the account directory is misconfigured: {0}")]
    DirectoryMisconfigured(String),
    #[error(transparent)]
    Directory(ProviderAccountDirectoryError),
    #[error("ai provider account storage error: {0}")]
    Storage(String),
}

/// Errors from a metered payer's usage source, such as GitHub's billing usage
/// API. Modeled on `ProviderAccountDirectoryError`; messages never carry
/// credentials.
#[derive(Debug, Clone, Error)]
pub enum MeteredUsageSourceError {
    /// The source rejected Toki's credentials.
    #[error("the usage source rejected Toki's credentials: {0}")]
    Unauthorized(String),
    /// The source refused, for example for a missing permission.
    #[error("the usage source refused: {0}")]
    Forbidden(String),
    /// The account or the organization does not exist.
    #[error("the usage source has no such account or organization: {0}")]
    NotFound(String),
    /// The source refused because of its rate limit, possibly saying for how
    /// long.
    #[error("the usage source is rate limiting requests")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    /// A timeout, a connection error or a server error.
    #[error("the usage source is unavailable: {0}")]
    Unavailable(String),
    /// A response that could not be read or failed validation.
    #[error("unexpected usage source response: {0}")]
    Response(String),
}

/// Errors of the metered usage import and its reconciliation.
#[derive(Debug, Error)]
pub enum AiMeteredUsageError {
    #[error("{0}")]
    Invalid(String),
    #[error("an import is already running")]
    ImportRunning,
    #[error("no usage source is configured")]
    SourceOff,
    #[error("the usage source is misconfigured: {0}")]
    SourceMisconfigured(String),
    #[error(transparent)]
    Source(MeteredUsageSourceError),
    #[error("user not found")]
    UserNotFound,
    #[error("ai metered usage storage error: {0}")]
    Storage(String),
}

impl From<AiProviderAccountError> for AiMeteredUsageError {
    fn from(error: AiProviderAccountError) -> Self {
        match error {
            AiProviderAccountError::Invalid(message) => Self::Invalid(message),
            AiProviderAccountError::UserNotFound => Self::UserNotFound,
            other => Self::Storage(other.to_string()),
        }
    }
}
