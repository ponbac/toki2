//! Admin-only provider accounts, under `/ai-usage/admin/provider-accounts`:
//! the accounts holding seats at a metered payer (GitHub Copilot), and whose
//! usage each one is.
//!
//! Every route needs admin power: an admin in a browser session, never an API
//! token (`auth::require_admin_power`). The routes are browser-only, off the
//! agent OpenAPI document and MCP, like the rest of admin billing.
//!
//! Dates are UTC calendar days as `YYYY-MM-DD`, both ends inclusive, and
//! instants are RFC 3339. Every malformed body or path is a `400` with the
//! usual JSON error.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    middleware,
    routing::{get, post, put},
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    adapters::inbound::http::{
        ai_billing::UsageResponse, ai_metered_usage::credits, ai_subscriptions::iso_date,
    },
    auth::{require_admin_power, AuthUser},
    domain::{
        models::{
            AiAcceptedSuggestions, AiAccountDisposition, AiAccountDispositionId,
            AiAccountDispositionTerms, AiAccountPeriod, AiAccountRow, AiAccountSuggestion,
            AiAccountTarget, AiDirectoryHealth, AiDirectoryRefresh, AiProviderAccountId,
            AiProviderAccountOverview, AiStoredSeat, AiSuggestionConfidence, AiSuggestionEvidence,
            AiSuggestionSkip, AiUnlinkedUser, AiUsageDateRange, AiUserActivity, UserId,
        },
        ports::inbound::AiProviderAccountService,
        AiProviderAccountError, ProviderAccountDirectoryError,
    },
    routes::ApiError,
};

/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;
/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiProviderAccountService>: FromRef<S>,
{
    Router::new()
        .route("/provider-accounts", get(overview))
        .route("/provider-accounts/refresh", post(refresh))
        .route("/provider-accounts/dispositions", post(create_disposition))
        .route(
            "/provider-accounts/dispositions/{disposition_id}",
            put(update_disposition).delete(delete_disposition),
        )
        .route(
            "/provider-accounts/accept-suggestions",
            post(accept_suggestions),
        )
        .route_layer(middleware::from_fn(require_admin_power))
}

impl From<AiProviderAccountError> for ApiError {
    fn from(err: AiProviderAccountError) -> Self {
        match err {
            AiProviderAccountError::Invalid(message) => Self::bad_request(message),
            AiProviderAccountError::NotFound
            | AiProviderAccountError::AccountNotFound
            | AiProviderAccountError::UserNotFound => Self::not_found(err.to_string()),
            AiProviderAccountError::AccountOverlap { .. }
            | AiProviderAccountError::UserOverlap { .. } => Self::conflict(err.to_string()),
            AiProviderAccountError::DirectoryOff
            | AiProviderAccountError::DirectoryMisconfigured(_) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string())
            }
            AiProviderAccountError::Directory(ProviderAccountDirectoryError::RateLimited {
                ..
            }) => Self::new(StatusCode::TOO_MANY_REQUESTS, err.to_string()),
            AiProviderAccountError::Directory(_) => {
                Self::new(StatusCode::BAD_GATEWAY, err.to_string())
            }
            AiProviderAccountError::Storage(message) => {
                tracing::error!("AI provider account operation failed: {}", message);
                Self::internal("ai provider account operation failed")
            }
        }
    }
}

/// Every account with its seats, dispositions and suggestions, and what needs
/// a look.
async fn overview(
    State(service): State<Arc<dyn AiProviderAccountService>>,
) -> Result<Json<ProviderAccountsResponse>, ApiError> {
    Ok(Json(service.overview().await?.into()))
}

/// Reads seats and profiles from the payer now. The refresh runs on its own
/// task, so a closed tab does not cancel it halfway.
async fn refresh(
    State(service): State<Arc<dyn AiProviderAccountService>>,
) -> Result<Json<RefreshResponse>, ApiError> {
    let refresh = tokio::spawn(async move { service.refresh().await })
        .await
        .map_err(|error| {
            tracing::error!(%error, "the provider account refresh task failed");
            ApiError::internal("the refresh did not finish")
        })??;
    Ok(Json(refresh.into()))
}

async fn create_disposition(
    user: AuthUser,
    State(service): State<Arc<dyn AiProviderAccountService>>,
    WithRejection(Json(request), _): JsonBody<CreateDispositionRequest>,
) -> Result<(StatusCode, Json<DispositionResponse>), ApiError> {
    let account = account_id(request.account_id)?;
    let terms = request.terms.into_terms()?;
    let disposition = service.create_disposition(account, terms, user.id).await?;
    Ok((StatusCode::CREATED, Json(disposition.into())))
}

async fn update_disposition(
    user: AuthUser,
    State(service): State<Arc<dyn AiProviderAccountService>>,
    WithRejection(Path(disposition_id), _): PathParam<i32>,
    WithRejection(Json(request), _): JsonBody<DispositionTermsRequest>,
) -> Result<Json<DispositionResponse>, ApiError> {
    let disposition = service
        .update_disposition(
            AiAccountDispositionId::new(disposition_id),
            request.into_terms()?,
            user.id,
        )
        .await?;
    Ok(Json(disposition.into()))
}

async fn delete_disposition(
    State(service): State<Arc<dyn AiProviderAccountService>>,
    WithRejection(Path(disposition_id), _): PathParam<i32>,
) -> Result<StatusCode, ApiError> {
    service
        .delete_disposition(AiAccountDispositionId::new(disposition_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn accept_suggestions(
    user: AuthUser,
    State(service): State<Arc<dyn AiProviderAccountService>>,
    WithRejection(Json(request), _): JsonBody<AcceptSuggestionsRequest>,
) -> Result<Json<AcceptedSuggestionsResponse>, ApiError> {
    let picks = request
        .picks
        .into_iter()
        .map(|pick| Ok((account_id(pick.account_id)?, UserId::new(pick.user_id))))
        .collect::<Result<Vec<_>, ApiError>>()?;
    let accepted = service.accept_suggestions(&picks, user.id).await?;
    Ok(Json(accepted.into()))
}

fn account_id(raw: i64) -> Result<AiProviderAccountId, ApiError> {
    AiProviderAccountId::new(raw)
        .ok_or_else(|| ApiError::bad_request(format!("accountId must be positive, got {raw}")))
}

/// Whose an account's usage is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TargetBody {
    /// A Toki user's.
    User { user_id: i32 },
    /// Not a Toki user's: Spinit overhead.
    External,
}

impl From<TargetBody> for AiAccountTarget {
    fn from(target: TargetBody) -> Self {
        match target {
            TargetBody::User { user_id } => Self::User(UserId::new(user_id)),
            TargetBody::External => Self::External,
        }
    }
}

impl From<AiAccountTarget> for TargetBody {
    fn from(target: AiAccountTarget) -> Self {
        match target {
            AiAccountTarget::User(user_id) => Self::User {
                user_id: user_id.as_i32(),
            },
            AiAccountTarget::External => Self::External,
        }
    }
}

/// A disposition's target and UTC days, both inclusive.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispositionTermsRequest {
    target: TargetBody,
    #[serde(with = "iso_date")]
    valid_from: Date,
    /// Null or absent while ongoing.
    #[serde(default, with = "iso_date::option")]
    valid_to: Option<Date>,
}

impl DispositionTermsRequest {
    fn into_terms(self) -> Result<AiAccountDispositionTerms, ApiError> {
        Ok(AiAccountDispositionTerms {
            target: self.target.into(),
            period: AiAccountPeriod::new(self.valid_from, self.valid_to)?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDispositionRequest {
    account_id: i64,
    #[serde(flatten)]
    terms: DispositionTermsRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptSuggestionsRequest {
    picks: Vec<PickRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickRequest {
    account_id: i64,
    user_id: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DispositionResponse {
    id: i32,
    account_id: i64,
    /// The account's login when the disposition was last written.
    login: String,
    target: TargetBody,
    #[serde(with = "iso_date")]
    valid_from: Date,
    #[serde(with = "iso_date::option")]
    valid_to: Option<Date>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    updated_by: Option<i32>,
}

impl From<AiAccountDisposition> for DispositionResponse {
    fn from(disposition: AiAccountDisposition) -> Self {
        Self {
            id: disposition.id.as_i32(),
            account_id: disposition.account_id.as_i64(),
            login: disposition.login.as_str().to_string(),
            target: disposition.terms.target.into(),
            valid_from: disposition.terms.period.valid_from(),
            valid_to: disposition.terms.period.valid_to(),
            updated_at: disposition.updated_at,
            updated_by: disposition.updated_by.map(|user_id| user_id.as_i32()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderAccountsResponse {
    /// The metered payer whose accounts these are, such as `copilot`.
    provider: &'static str,
    directory: DirectoryResponse,
    /// Today's UTC date.
    #[serde(with = "iso_date")]
    today: Date,
    /// The first day of the review window: the first day of the previous UTC
    /// month.
    #[serde(with = "iso_date")]
    review_from: Date,
    accounts: Vec<AccountResponse>,
    /// Each user's usage the payer paid for in the review window.
    activity: Vec<ActivityResponse>,
    /// Users with such usage on days without a link.
    unlinked_users: Vec<UnlinkedUserResponse>,
    /// The UTC month of each account's `creditsThisMonth`, such as `2026-10`.
    credits_month: String,
}

impl From<AiProviderAccountOverview> for ProviderAccountsResponse {
    fn from(overview: AiProviderAccountOverview) -> Self {
        let directory = overview.directory;
        let (status, message) = match directory.health {
            AiDirectoryHealth::Ready => (DirectoryStatus::Ready, None),
            AiDirectoryHealth::Disabled => (DirectoryStatus::Disabled, None),
            AiDirectoryHealth::Misconfigured(message) => {
                (DirectoryStatus::Misconfigured, Some(message))
            }
        };
        let refreshes = directory.status;
        let accounts = overview.overview;
        Self {
            provider: overview.payer.as_str(),
            directory: DirectoryResponse {
                status,
                message,
                namespace: directory.namespace,
                last_attempt_at: refreshes.as_ref().map(|status| status.last_attempt_at),
                last_success_at: refreshes.as_ref().and_then(|status| status.last_success_at),
                last_error: refreshes
                    .as_ref()
                    .and_then(|status| status.last_error.clone()),
                seats_listed: refreshes.as_ref().and_then(|status| status.seats_listed),
                seats_reported: refreshes.as_ref().and_then(|status| status.seats_reported),
            },
            today: accounts.today,
            review_from: accounts.review_from,
            accounts: accounts.accounts.into_iter().map(Into::into).collect(),
            activity: accounts
                .activity
                .iter()
                .map(|(user_id, activity)| ActivityResponse::new(*user_id, activity))
                .collect(),
            unlinked_users: accounts
                .unlinked_users
                .into_iter()
                .map(Into::into)
                .collect(),
            credits_month: accounts.credits_month.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum DirectoryStatus {
    Ready,
    Disabled,
    Misconfigured,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryResponse {
    status: DirectoryStatus,
    /// Why the directory is misconfigured; null otherwise.
    message: Option<String>,
    /// Where the accounts live, such as the GitHub organization.
    namespace: String,
    #[serde(with = "time::serde::rfc3339::option")]
    last_attempt_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    last_success_at: Option<OffsetDateTime>,
    last_error: Option<String>,
    seats_listed: Option<u32>,
    seats_reported: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
    account_id: i64,
    login: String,
    profile: Option<ProfileResponse>,
    /// Oldest first.
    seats: Vec<SeatResponse>,
    dispositions: Vec<DispositionResponse>,
    current_disposition_id: Option<i32>,
    uncovered_seat_days: Vec<DayRangeResponse>,
    /// The account's gross credits this UTC month, as hundredths of a credit.
    credits_this_month: String,
    /// Review window days with credits that no disposition covers.
    uncovered_credit_days: Vec<DayRangeResponse>,
    /// Review window days with credits outside every seat.
    credit_days_without_seat: u32,
    needs_look: bool,
    suggestions: Vec<SuggestionResponse>,
    default_period: Option<PeriodResponse>,
}

impl From<AiAccountRow> for AccountResponse {
    fn from(row: AiAccountRow) -> Self {
        let account = row.account;
        Self {
            account_id: account.account_id.as_i64(),
            login: account.login.as_str().to_string(),
            profile: account.profile.map(|profile| ProfileResponse {
                name: profile.name,
                email: profile.email,
                fetched_at: profile.fetched_at,
            }),
            seats: account.seats.iter().map(SeatResponse::from).collect(),
            dispositions: account.dispositions.into_iter().map(Into::into).collect(),
            current_disposition_id: row.current_disposition.map(|id| id.as_i32()),
            uncovered_seat_days: row
                .uncovered_seat_days
                .into_iter()
                .map(DayRangeResponse::from)
                .collect(),
            credits_this_month: credits(row.credits_this_month),
            uncovered_credit_days: row
                .uncovered_credit_days
                .into_iter()
                .map(DayRangeResponse::from)
                .collect(),
            credit_days_without_seat: row.credit_days_without_seat,
            needs_look: row.needs_look,
            suggestions: row.suggestions.into_iter().map(Into::into).collect(),
            default_period: row.default_period.map(|period| PeriodResponse {
                valid_from: period.valid_from(),
                valid_to: period.valid_to(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileResponse {
    name: Option<String>,
    /// The public email only.
    email: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    fetched_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeatResponse {
    /// The payer's assignment instant.
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// Its UTC day, the seat's first.
    #[serde(with = "iso_date")]
    first_day: Date,
    #[serde(with = "iso_date::option")]
    pending_cancellation_on: Option<Date>,
    #[serde(with = "time::serde::rfc3339::option")]
    last_activity_at: Option<OffsetDateTime>,
    last_activity_client: Option<String>,
    /// Whether the payer still lists the seat.
    listed: bool,
    /// The seat's last day, inclusive, once it is no longer listed.
    #[serde(with = "iso_date::option")]
    last_day: Option<Date>,
}

impl From<&AiStoredSeat> for SeatResponse {
    fn from(seat: &AiStoredSeat) -> Self {
        Self {
            created_at: seat.created_at,
            first_day: seat.first_day(),
            pending_cancellation_on: seat.pending_cancellation_on,
            last_activity_at: seat.last_activity_at,
            last_activity_client: seat.last_activity_client.clone(),
            listed: seat.is_listed(),
            last_day: seat.last_day(),
        }
    }
}

/// Days from `from` through `through`, both inclusive.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayRangeResponse {
    #[serde(with = "iso_date")]
    from: Date,
    #[serde(with = "iso_date")]
    through: Date,
}

impl From<AiUsageDateRange> for DayRangeResponse {
    fn from(range: AiUsageDateRange) -> Self {
        Self {
            from: range.start(),
            through: range.last_day(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PeriodResponse {
    #[serde(with = "iso_date")]
    valid_from: Date,
    #[serde(with = "iso_date::option")]
    valid_to: Option<Date>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionResponse {
    user_id: i32,
    confidence: ConfidenceResponse,
    evidence: Vec<EvidenceResponse>,
}

impl From<AiAccountSuggestion> for SuggestionResponse {
    fn from(suggestion: AiAccountSuggestion) -> Self {
        Self {
            user_id: suggestion.user_id.as_i32(),
            confidence: match suggestion.confidence {
                AiSuggestionConfidence::Verified => ConfidenceResponse::Verified,
                AiSuggestionConfidence::Probable => ConfidenceResponse::Probable,
            },
            evidence: suggestion.evidence.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ConfidenceResponse {
    Verified,
    Probable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum EvidenceResponse {
    Email,
    Name,
    NameFirstLast,
    NameInitial,
    NameFirstOnly,
    LoginFullName,
    LoginInitialSurname,
    LoginFirstSurnamePrefix,
    LoginFirstOnly,
}

impl From<AiSuggestionEvidence> for EvidenceResponse {
    fn from(evidence: AiSuggestionEvidence) -> Self {
        match evidence {
            AiSuggestionEvidence::Email => Self::Email,
            AiSuggestionEvidence::Name => Self::Name,
            AiSuggestionEvidence::NameFirstLast => Self::NameFirstLast,
            AiSuggestionEvidence::NameInitial => Self::NameInitial,
            AiSuggestionEvidence::NameFirstOnly => Self::NameFirstOnly,
            AiSuggestionEvidence::LoginFullName => Self::LoginFullName,
            AiSuggestionEvidence::LoginInitialSurname => Self::LoginInitialSurname,
            AiSuggestionEvidence::LoginFirstSurnamePrefix => Self::LoginFirstSurnamePrefix,
            AiSuggestionEvidence::LoginFirstOnly => Self::LoginFirstOnly,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityResponse {
    user_id: i32,
    /// Days with usage the payer paid for.
    days: u32,
    #[serde(with = "iso_date::option")]
    last_day: Option<Date>,
    usage: UsageResponse,
}

impl ActivityResponse {
    fn new(user_id: UserId, activity: &AiUserActivity) -> Self {
        Self {
            user_id: user_id.as_i32(),
            days: activity.days,
            last_day: activity.last_day,
            usage: (&activity.usage).into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnlinkedUserResponse {
    user_id: i32,
    /// Days with usage that none of the user's dispositions covers.
    uncovered_days: u32,
    #[serde(with = "iso_date")]
    first_day: Date,
    #[serde(with = "iso_date")]
    last_day: Date,
    /// The usage on those days.
    usage: UsageResponse,
    /// Accounts whose top suggestion is the user.
    suggested_account_ids: Vec<i64>,
}

impl From<AiUnlinkedUser> for UnlinkedUserResponse {
    fn from(user: AiUnlinkedUser) -> Self {
        Self {
            user_id: user.user_id.as_i32(),
            uncovered_days: user.uncovered_days,
            first_day: user.first_day,
            last_day: user.last_day,
            usage: (&user.usage).into(),
            suggested_account_ids: user
                .suggested_accounts
                .into_iter()
                .map(AiProviderAccountId::as_i64)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshResponse {
    #[serde(with = "time::serde::rfc3339")]
    started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    finished_at: OffsetDateTime,
    seats_listed: u32,
    seats_reported: Option<u32>,
    /// Whether the listing was complete, so seats it omitted were ended.
    complete: bool,
    seats_started: u32,
    seats_ended: u32,
    profiles_read: u32,
    profiles_failed: u32,
}

impl From<AiDirectoryRefresh> for RefreshResponse {
    fn from(refresh: AiDirectoryRefresh) -> Self {
        Self {
            started_at: refresh.started_at,
            finished_at: refresh.finished_at,
            seats_listed: refresh.seats_listed,
            seats_reported: refresh.seats_reported,
            complete: refresh.complete,
            seats_started: refresh.seats_started,
            seats_ended: refresh.seats_ended,
            profiles_read: refresh.profiles_read,
            profiles_failed: refresh.profiles_failed,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedSuggestionsResponse {
    created: Vec<DispositionResponse>,
    skipped: Vec<SkippedSuggestionResponse>,
}

impl From<AiAcceptedSuggestions> for AcceptedSuggestionsResponse {
    fn from(accepted: AiAcceptedSuggestions) -> Self {
        Self {
            created: accepted.created.into_iter().map(Into::into).collect(),
            skipped: accepted
                .skipped
                .into_iter()
                .map(|(account_id, reason)| SkippedSuggestionResponse {
                    account_id: account_id.as_i64(),
                    reason: match reason {
                        AiSuggestionSkip::NotVerified => SkipReason::NotVerified,
                        AiSuggestionSkip::Changed => SkipReason::Changed,
                        AiSuggestionSkip::AlreadyDisposed => SkipReason::AlreadyDisposed,
                        AiSuggestionSkip::AccountOverlap => SkipReason::AccountOverlap,
                        AiSuggestionSkip::UserOverlap => SkipReason::UserOverlap,
                    },
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkippedSuggestionResponse {
    account_id: i64,
    reason: SkipReason,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum SkipReason {
    /// The user is the account's top suggestion, but only a probable one.
    NotVerified,
    /// The account's top suggestion changed, or there is none.
    Changed,
    /// The account already has a disposition.
    AlreadyDisposed,
    AccountOverlap,
    UserOverlap,
}

#[cfg(test)]
mod tests;
