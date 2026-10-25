//! Admin-only metered usage, under `/ai-usage/admin/metered-usage`: the GitHub
//! Copilot import and its reconciliation with Toki's tracked usage.
//!
//! Every route needs admin power: an admin in a browser session, never an API
//! token (`auth::require_admin_power`). The routes are browser-only, off the
//! agent OpenAPI document and MCP, like the rest of admin billing.
//!
//! - Credits are decimal strings with exactly two decimals: hundredths of a
//!   credit, rounded half up from nanodollars (a credit is $0.01). Only the
//!   residual and the `unknownAccount*` / `incompleteImport` totals can be
//!   negative.
//! - USD amounts are strings with two decimals; GitHub's raw quantities are
//!   exact decimal strings.
//! - Dates are UTC calendar days as `YYYY-MM-DD`; instants are RFC 3339.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    adapters::inbound::http::ai_subscriptions::iso_date,
    auth::{require_admin_power, AuthUser},
    domain::{
        models::{
            AiAccountImportError, AiAccountReconciliation, AiAccountTarget, AiBillingMonth,
            AiCreditKind, AiDecimal, AiGroupKind, AiImportFreshness, AiImportKind, AiImportRun,
            AiMeteredModelStatus, AiMeteredMonth, AiMeteredSource, AiMeteredSourceHealth,
            AiMonthMode, AiOrgChargeKind, AiReconciliationFlag, AiReconciliationGroup,
            AiResidualStatus, AiRunStatus, AiSourceState, AiStaleReason, AiUsageCheck, AiUsdNanos,
            UserId, NANOS_PER_CREDIT,
        },
        ports::inbound::AiMeteredUsageService,
        AiMeteredUsageError, MeteredUsageSourceError,
    },
    routes::ApiError,
};

/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;
/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;

/// Nanodollars per hundredth of a credit.
const NANOS_PER_CREDIT_HUNDREDTH: i64 = NANOS_PER_CREDIT / 100;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiMeteredUsageService>: FromRef<S>,
{
    Router::new()
        .route("/metered-usage/{month}", get(month_view))
        .route(
            "/metered-usage/{month}/developers/{user_id}",
            get(developer_view),
        )
        .route("/metered-usage/{month}/import", post(start_import))
        .route_layer(middleware::from_fn(require_admin_power))
}

impl From<AiMeteredUsageError> for ApiError {
    fn from(err: AiMeteredUsageError) -> Self {
        match err {
            AiMeteredUsageError::Invalid(message) => Self::bad_request(message),
            AiMeteredUsageError::ImportRunning => Self::conflict(err.to_string()),
            AiMeteredUsageError::SourceOff | AiMeteredUsageError::SourceMisconfigured(_) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string())
            }
            AiMeteredUsageError::Source(MeteredUsageSourceError::RateLimited { .. }) => {
                Self::new(StatusCode::TOO_MANY_REQUESTS, err.to_string())
            }
            AiMeteredUsageError::Source(_) => Self::new(StatusCode::BAD_GATEWAY, err.to_string()),
            AiMeteredUsageError::UserNotFound => Self::not_found(err.to_string()),
            AiMeteredUsageError::Storage(message) => {
                tracing::error!("AI metered usage operation failed: {}", message);
                Self::internal("ai metered usage operation failed")
            }
        }
    }
}

fn parse_month(raw: &str) -> Result<AiBillingMonth, ApiError> {
    AiBillingMonth::parse(raw)
        .ok_or_else(|| ApiError::bad_request(format!("expected a month as YYYY-MM, got {raw:?}")))
}

/// Every account's reconciliation of a UTC month, with the import's state.
async fn month_view(
    State(service): State<Arc<dyn AiMeteredUsageService>>,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Json<MeteredMonthResponse>, ApiError> {
    let month = parse_month(&month)?;
    Ok(Json(service.reconciliation(month).await?.into()))
}

/// One user's linked accounts in a UTC month.
async fn developer_view(
    State(service): State<Arc<dyn AiMeteredUsageService>>,
    WithRejection(Path((month, user_id)), _): PathParam<(String, i32)>,
) -> Result<Json<MeteredMonthResponse>, ApiError> {
    let month = parse_month(&month)?;
    Ok(Json(
        service
            .developer_reconciliation(month, UserId::new(user_id))
            .await?
            .into(),
    ))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ModeBody {
    Changes,
    Full,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportRequest {
    mode: ModeBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportStartedResponse {
    run_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    started_at: OffsetDateTime,
    months: Vec<String>,
}

/// Starts an import of `month` (plus months with gaps) and answers at once;
/// the run continues on its own task, so a closed tab never cancels it.
async fn start_import(
    user: AuthUser,
    State(service): State<Arc<dyn AiMeteredUsageService>>,
    WithRejection(Path(month), _): PathParam<String>,
    WithRejection(Json(request), _): JsonBody<ImportRequest>,
) -> Result<(StatusCode, Json<ImportStartedResponse>), ApiError> {
    let month = parse_month(&month)?;
    let mode = match request.mode {
        ModeBody::Changes => AiMonthMode::Changes,
        ModeBody::Full => AiMonthMode::Full,
    };
    let ticket = service.begin_import(month, mode, user.id).await?;
    let response = ImportStartedResponse {
        run_id: ticket.run_id.as_i64(),
        started_at: ticket.started_at,
        months: ticket
            .plan
            .months
            .iter()
            .map(|(month, _)| month.to_string())
            .collect(),
    };
    tokio::spawn(async move {
        match service.run_import(ticket).await {
            Ok(run) => tracing::info!(
                status = run.status.as_str(),
                requests = run.requests,
                "an admin's metered usage import finished"
            ),
            Err(error) => tracing::warn!(%error, "an admin's metered usage import failed"),
        }
    });
    Ok((StatusCode::ACCEPTED, Json(response)))
}

/// Hundredths of a credit, rounded half up (away from zero for negative
/// amounts), as `12.34` or `-0.05`.
fn signed_credits(nanos: i64) -> String {
    let magnitude = nanos.unsigned_abs();
    let step = NANOS_PER_CREDIT_HUNDREDTH as u64;
    let hundredths = magnitude / step + u64::from(magnitude % step >= step / 2);
    let sign = if nanos < 0 && hundredths > 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", hundredths / 100, hundredths % 100)
}

/// An amount in credits, as hundredths of a credit: `12.34`.
pub(crate) fn credits(nanos: AiUsdNanos) -> String {
    signed_credits(nanos.nanos())
}

fn ratio(numerator: AiUsdNanos, denominator: AiUsdNanos) -> Option<f64> {
    (denominator.nanos() > 0).then(|| numerator.nanos() as f64 / denominator.nanos() as f64)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeteredMonthResponse {
    /// The metered payer, such as `copilot`.
    provider: &'static str,
    /// Where its accounts live, such as the GitHub organization.
    namespace: String,
    month: String,
    #[serde(with = "iso_date")]
    today: Date,
    /// Months with imported data, oldest first.
    months: Vec<String>,
    import: ImportResponse,
    pool: Option<PoolResponse>,
    totals: TotalsResponse,
    accounts: Vec<AccountResponse>,
    unlinked_tracked: Vec<UnlinkedTrackedResponse>,
    repositories: Vec<RepositoryResponse>,
    code_review: Option<CodeReviewResponse>,
    residual_days: Vec<ResidualDayResponse>,
    /// The organization's stored month against the payer's month total, which
    /// the residual rests on; null in one user's view and before any check.
    org_check: Option<CheckResponse>,
}

impl From<AiMeteredMonth> for MeteredMonthResponse {
    fn from(month: AiMeteredMonth) -> Self {
        let reconciliation = month.reconciliation;
        let totals = reconciliation.totals;
        Self {
            provider: month.payer.as_str(),
            namespace: month.namespace,
            month: reconciliation.month.to_string(),
            today: month.today,
            months: month.months.iter().map(ToString::to_string).collect(),
            import: month.import.into(),
            pool: reconciliation.pool.map(|pool| PoolResponse {
                fetched_at: pool.fetched_at,
                exhausted_on: pool.exhausted_on,
                credits: PoolCreditsResponse {
                    gross_credits: pool.credits.gross_quantity.to_fixed(2),
                    discount_credits: pool.credits.discount_quantity.to_fixed(2),
                    net_credits: pool.credits.net_quantity.to_fixed(2),
                    gross_usd: pool.credits.gross_amount.to_fixed(2),
                    discount_usd: pool.credits.discount_amount.to_fixed(2),
                    net_usd: pool.credits.net_amount.to_fixed(2),
                },
                seats: pool.seats.map(|seats| SeatsResponse {
                    user_months: seats.user_months.to_string(),
                    gross_usd: seats.gross_amount.to_fixed(2),
                    net_usd: seats.net_amount.to_fixed(2),
                }),
                payer_net_usd: pool.net_total.to_fixed(2),
                unrecognized: pool
                    .unrecognized
                    .into_iter()
                    .map(|(product, sku, unit)| UnrecognizedResponse { product, sku, unit })
                    .collect(),
            }),
            totals: TotalsResponse {
                gross: credits(totals.gross),
                repository_billed: credits(totals.repository_billed),
                tracked: credits(totals.tracked),
                tracked_old_basis: credits(totals.tracked_old_basis),
                tracked_before_floor: credits(totals.tracked_before_floor),
                attributed: credits(totals.attributed),
                untracked: credits(totals.untracked),
                unlinked: credits(totals.unlinked),
                external: credits(totals.external),
                unknown_account: signed_credits(totals.unknown_account),
                unknown_account_settled: signed_credits(totals.unknown_account_settled),
                incomplete_import: signed_credits(totals.incomplete_import),
            },
            accounts: reconciliation
                .accounts
                .into_iter()
                .map(Into::into)
                .collect(),
            unlinked_tracked: reconciliation
                .unlinked_tracked
                .into_iter()
                .map(|user| UnlinkedTrackedResponse {
                    user_id: user.user_id.as_i32(),
                    tracked: credits(user.tracked),
                    days: user.days,
                })
                .collect(),
            repositories: reconciliation
                .repositories
                .into_iter()
                .map(|usage| RepositoryResponse {
                    repository: usage.repository,
                    product: usage.product,
                    sku: usage.sku,
                    unit: usage.unit,
                    kind: usage.kind.into(),
                    quantity: usage.quantity.to_string(),
                    credits: usage.credit_usd.map(credits),
                    gross_usd: usage.gross_amount.to_fixed(2),
                    discount_usd: usage.discount_amount.to_fixed(2),
                    net_usd: usage.net_amount.to_fixed(2),
                    days: usage.days,
                })
                .collect(),
            code_review: reconciliation.code_review.map(|check| CodeReviewResponse {
                by_accounts: credits(check.by_accounts),
                by_repositories: credits(check.by_repositories),
            }),
            residual_days: reconciliation
                .residual_days
                .into_iter()
                .map(|day| ResidualDayResponse {
                    day: day.day,
                    org_gross: credits(day.org_gross),
                    accounts_gross: credits(day.accounts_gross),
                    residual: signed_credits(day.residual),
                    status: match day.status {
                        AiResidualStatus::Settled => ResidualStatusResponse::Settled,
                        AiResidualStatus::Provisional => ResidualStatusResponse::Provisional,
                        AiResidualStatus::Incomplete => ResidualStatusResponse::Incomplete,
                    },
                    accounts_exceed_org: day.accounts_exceed_org,
                    groups: day
                        .groups
                        .into_iter()
                        .map(|group| ResidualGroupResponse {
                            sku: group.sku,
                            model: group.model,
                            residual: signed_credits(group.residual),
                        })
                        .collect(),
                })
                .collect(),
            org_check: reconciliation.org_check.map(Into::into),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum SourceHealthResponse {
    Ready,
    Disabled,
    Misconfigured,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum FreshnessResponse {
    Fresh,
    Stale,
    Never,
    Off,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResponse {
    status: SourceHealthResponse,
    /// Why the source is misconfigured; null otherwise.
    message: Option<String>,
    #[serde(with = "iso_date")]
    backfill_from: Date,
    /// Whether scheduled imports run in this build.
    unattended: bool,
    freshness: FreshnessResponse,
    /// Why a stale import is stale, for the page to phrase; null otherwise.
    stale_reason: Option<StaleReasonResponse>,
    running: Option<RunResponse>,
    last_run: Option<RunResponse>,
    account_errors: Vec<AccountErrorResponse>,
    sources: Vec<SourceResponse>,
}

impl From<crate::domain::models::AiImportOverview> for ImportResponse {
    fn from(overview: crate::domain::models::AiImportOverview) -> Self {
        let (status, message) = match overview.health {
            AiMeteredSourceHealth::Ready => (SourceHealthResponse::Ready, None),
            AiMeteredSourceHealth::Disabled => (SourceHealthResponse::Disabled, None),
            AiMeteredSourceHealth::Misconfigured(message) => {
                (SourceHealthResponse::Misconfigured, Some(message))
            }
        };
        let (freshness, stale_reason) = match overview.freshness {
            AiImportFreshness::Fresh => (FreshnessResponse::Fresh, None),
            AiImportFreshness::Stale { reason } => (FreshnessResponse::Stale, Some(reason.into())),
            AiImportFreshness::Never => (FreshnessResponse::Never, None),
            AiImportFreshness::Off => (FreshnessResponse::Off, None),
        };
        Self {
            status,
            message,
            backfill_from: overview.backfill_from,
            unattended: overview.unattended,
            freshness,
            stale_reason,
            running: overview.running.map(Into::into),
            last_run: overview.last_run.map(Into::into),
            account_errors: overview
                .account_errors
                .into_iter()
                .map(AccountErrorResponse::from)
                .collect(),
            sources: overview
                .sources
                .into_iter()
                .map(SourceResponse::from)
                .collect(),
        }
    }
}

/// Why the import is stale.
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum StaleReasonResponse {
    LastRunFailed {
        error: String,
    },
    NeverSucceeded,
    LastSuccessOld {
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    SourceNotImported {
        source: SourceKindResponse,
    },
    SourceBehind {
        source: SourceKindResponse,
        /// The last UTC day imported.
        #[serde(with = "iso_date")]
        through: Date,
    },
    SummaryNeverFetched,
    SummaryOld {
        #[serde(with = "time::serde::rfc3339")]
        at: OffsetDateTime,
    },
    AccountFailed {
        login: String,
        month: String,
        error: String,
    },
}

impl From<AiStaleReason> for StaleReasonResponse {
    fn from(reason: AiStaleReason) -> Self {
        match reason {
            AiStaleReason::LastRunFailed { error } => Self::LastRunFailed { error },
            AiStaleReason::NeverSucceeded => Self::NeverSucceeded,
            AiStaleReason::LastSuccessOld { at } => Self::LastSuccessOld { at },
            AiStaleReason::SourceNotImported { source } => Self::SourceNotImported {
                source: source.into(),
            },
            AiStaleReason::SourceBehind { source, through } => Self::SourceBehind {
                source: source.into(),
                through,
            },
            AiStaleReason::SummaryNeverFetched => Self::SummaryNeverFetched,
            AiStaleReason::SummaryOld { at } => Self::SummaryOld { at },
            AiStaleReason::AccountFailed {
                login,
                month,
                error,
            } => Self::AccountFailed {
                login: login.as_str().to_string(),
                month: month.to_string(),
                error,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountErrorResponse {
    login: String,
    /// The UTC month whose import failed: one entry per account and month.
    month: String,
    error: String,
    /// Whether the account held a seat in that month; its own errors without
    /// one are informational.
    seated: bool,
    /// Whether a run stopped before finishing the account's month: no
    /// answer of its own, so it counts whatever the seat.
    interrupted: bool,
}

impl From<AiAccountImportError> for AccountErrorResponse {
    fn from(error: AiAccountImportError) -> Self {
        Self {
            login: error.login.as_str().to_string(),
            month: error.month.to_string(),
            error: error.error,
            seated: error.seated,
            interrupted: error.interrupted,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum SourceKindResponse {
    AccountCredits,
    OrgCredits,
    OrgUsage,
    Summary,
}

impl From<AiMeteredSource> for SourceKindResponse {
    fn from(source: AiMeteredSource) -> Self {
        match source {
            AiMeteredSource::AccountCredits => Self::AccountCredits,
            AiMeteredSource::OrgCredits => Self::OrgCredits,
            AiMeteredSource::OrgUsage => Self::OrgUsage,
            AiMeteredSource::Summary => Self::Summary,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceResponse {
    source: SourceKindResponse,
    #[serde(with = "iso_date::option")]
    imported_through: Option<Date>,
    #[serde(with = "time::serde::rfc3339::option")]
    last_success_at: Option<OffsetDateTime>,
    last_error: Option<String>,
    settle_lag_days: Option<u32>,
}

impl From<AiSourceState> for SourceResponse {
    fn from(state: AiSourceState) -> Self {
        Self {
            source: state.source.into(),
            imported_through: state.imported_through,
            last_success_at: state.last_success_at,
            last_error: state.last_error,
            settle_lag_days: state.settle_lag_days,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum RunKindResponse {
    Hourly,
    Nightly,
    Admin,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum RunStatusResponse {
    Running,
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    run_id: i64,
    kind: RunKindResponse,
    months: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    finished_at: Option<OffsetDateTime>,
    status: RunStatusResponse,
    requests: u32,
    slices_fetched: u32,
    slices_changed: u32,
    account_errors: u32,
    error: Option<String>,
}

impl From<AiImportRun> for RunResponse {
    fn from(run: AiImportRun) -> Self {
        Self {
            run_id: run.id.as_i64(),
            kind: match run.kind {
                AiImportKind::Hourly => RunKindResponse::Hourly,
                AiImportKind::Nightly => RunKindResponse::Nightly,
                AiImportKind::Admin(_) => RunKindResponse::Admin,
            },
            months: run
                .months
                .iter()
                .map(|(month, _)| month.to_string())
                .collect(),
            started_at: run.started_at,
            finished_at: run.finished_at,
            status: match run.status {
                AiRunStatus::Running => RunStatusResponse::Running,
                AiRunStatus::Succeeded => RunStatusResponse::Succeeded,
                AiRunStatus::Partial => RunStatusResponse::Partial,
                AiRunStatus::Failed => RunStatusResponse::Failed,
            },
            requests: run.requests,
            slices_fetched: run.slices_fetched,
            slices_changed: run.slices_changed,
            account_errors: run.account_errors,
            error: run.error,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PoolCreditsResponse {
    gross_credits: String,
    discount_credits: String,
    net_credits: String,
    gross_usd: String,
    discount_usd: String,
    net_usd: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeatsResponse {
    user_months: String,
    gross_usd: String,
    net_usd: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnrecognizedResponse {
    product: String,
    sku: String,
    unit: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PoolResponse {
    #[serde(with = "time::serde::rfc3339")]
    fetched_at: OffsetDateTime,
    /// The first UTC day with net credits: the pool ran out.
    #[serde(with = "iso_date::option")]
    exhausted_on: Option<Date>,
    credits: PoolCreditsResponse,
    seats: Option<SeatsResponse>,
    /// The payer's bill for the month so far: every row's net amount.
    payer_net_usd: String,
    unrecognized: Vec<UnrecognizedResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsResponse {
    gross: String,
    repository_billed: String,
    tracked: String,
    tracked_old_basis: String,
    tracked_before_floor: String,
    attributed: String,
    untracked: String,
    unlinked: String,
    external: String,
    unknown_account: String,
    unknown_account_settled: String,
    incomplete_import: String,
}

/// Whose an account's usage is.
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum TargetResponse {
    User { user_id: i32 },
    External,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnerResponse {
    target: TargetResponse,
    #[serde(with = "iso_date")]
    from: Date,
    #[serde(with = "iso_date")]
    through: Date,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum FlagResponse {
    TrackedNotBilled,
    LowRatio,
    StalePricing,
    UnknownModel,
    Unlinked,
    External,
    PayerTotalMismatch,
    ImportFailed,
}

impl From<AiReconciliationFlag> for FlagResponse {
    fn from(flag: AiReconciliationFlag) -> Self {
        match flag {
            AiReconciliationFlag::TrackedNotBilled => Self::TrackedNotBilled,
            AiReconciliationFlag::LowRatio => Self::LowRatio,
            AiReconciliationFlag::StalePricing => Self::StalePricing,
            AiReconciliationFlag::UnknownModel => Self::UnknownModel,
            AiReconciliationFlag::Unlinked => Self::Unlinked,
            AiReconciliationFlag::External => Self::External,
            AiReconciliationFlag::PayerTotalMismatch => Self::PayerTotalMismatch,
            AiReconciliationFlag::ImportFailed => Self::ImportFailed,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResponse {
    #[serde(with = "time::serde::rfc3339")]
    checked_at: OffsetDateTime,
    /// The payer's month total of gross quantities, exactly.
    payer_gross_quantity: Option<String>,
    stored_gross_quantity: String,
    matches: Option<bool>,
    error: Option<String>,
}

impl From<AiUsageCheck> for CheckResponse {
    fn from(check: AiUsageCheck) -> Self {
        Self {
            checked_at: check.checked_at,
            payer_gross_quantity: check.payer_gross.as_ref().map(AiDecimal::to_string),
            stored_gross_quantity: check.stored_gross.to_string(),
            matches: check.matches,
            error: check.error,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum GroupKindResponse {
    Trackable,
    Untrackable,
    RepositoryBilled,
    Unrecognized,
    TrackedOnly,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ModelStatusResponse {
    Confirmed,
    Provisional,
    Free,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupResponse {
    sku: Option<String>,
    model: String,
    labels: Vec<String>,
    kind: GroupKindResponse,
    status: Option<ModelStatusResponse>,
    gross: String,
    auto_gross: String,
    tracked: String,
    attributed: String,
    untracked: String,
    /// `G / T` on linked days; null without tracked usage.
    ratio: Option<f64>,
    flags: Vec<FlagResponse>,
}

impl From<AiReconciliationGroup> for GroupResponse {
    fn from(group: AiReconciliationGroup) -> Self {
        Self {
            sku: group.sku,
            model: group.model,
            labels: group.labels,
            kind: match group.kind {
                AiGroupKind::Credits(AiCreditKind::Trackable) => GroupKindResponse::Trackable,
                AiGroupKind::Credits(AiCreditKind::Untrackable) => GroupKindResponse::Untrackable,
                AiGroupKind::Credits(AiCreditKind::RepositoryBilled) => {
                    GroupKindResponse::RepositoryBilled
                }
                AiGroupKind::Credits(AiCreditKind::Unrecognized) => GroupKindResponse::Unrecognized,
                AiGroupKind::TrackedOnly => GroupKindResponse::TrackedOnly,
            },
            status: group.status.map(|status| match status {
                AiMeteredModelStatus::Confirmed => ModelStatusResponse::Confirmed,
                AiMeteredModelStatus::Provisional => ModelStatusResponse::Provisional,
                AiMeteredModelStatus::Free => ModelStatusResponse::Free,
            }),
            gross: credits(group.gross),
            auto_gross: credits(group.auto_gross),
            tracked: credits(group.tracked),
            attributed: credits(group.attributed),
            untracked: credits(group.untracked),
            ratio: ratio(group.linked_gross, group.tracked),
            flags: group.flags.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
    account_id: i64,
    login: String,
    owners: Vec<OwnerResponse>,
    /// Days with credits and no disposition.
    unlinked_days: u32,
    /// Days whose usage was not imported for the account: nothing compared.
    days_not_imported: u32,
    gross: String,
    repository_billed: String,
    untrackable: String,
    tracked: String,
    tracked_old_basis: String,
    tracked_before_floor: String,
    unpriced_records: i64,
    attributed: String,
    untracked: String,
    unlinked: String,
    external: String,
    /// Attributed over trackable gross on linked days; null without it.
    tracked_share: Option<f64>,
    flags: Vec<FlagResponse>,
    payer_check: Option<CheckResponse>,
    groups: Vec<GroupResponse>,
}

impl From<AiAccountReconciliation> for AccountResponse {
    fn from(account: AiAccountReconciliation) -> Self {
        Self {
            account_id: account.account_id.as_i64(),
            login: account.login,
            owners: account
                .owners
                .into_iter()
                .map(|(target, range)| OwnerResponse {
                    target: match target {
                        AiAccountTarget::User(user_id) => TargetResponse::User {
                            user_id: user_id.as_i32(),
                        },
                        AiAccountTarget::External => TargetResponse::External,
                    },
                    from: range.start(),
                    through: range.last_day(),
                })
                .collect(),
            unlinked_days: account.unlinked_days,
            days_not_imported: account.days_not_imported,
            gross: credits(account.gross),
            repository_billed: credits(account.repository_billed),
            untrackable: credits(account.untrackable),
            tracked: credits(account.tracked),
            tracked_old_basis: credits(account.tracked_old_basis),
            tracked_before_floor: credits(account.tracked_before_floor),
            unpriced_records: account.unpriced_records,
            attributed: credits(account.attributed),
            untracked: credits(account.untracked),
            unlinked: credits(account.unlinked),
            external: credits(account.external),
            tracked_share: account
                .tracked_share
                .map(|(numerator, denominator)| numerator as f64 / denominator as f64),
            flags: account.flags.into_iter().map(Into::into).collect(),
            payer_check: account.check.map(Into::into),
            groups: account.groups.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnlinkedTrackedResponse {
    user_id: i32,
    tracked: String,
    days: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum ChargeKindResponse {
    RepositoryCredits,
    AccountCreditsAggregate,
    UntrackableCredits,
    Compute,
    Seats,
    Unrecognized,
}

impl From<AiOrgChargeKind> for ChargeKindResponse {
    fn from(kind: AiOrgChargeKind) -> Self {
        match kind {
            AiOrgChargeKind::RepositoryCredits => Self::RepositoryCredits,
            AiOrgChargeKind::AccountCreditsAggregate => Self::AccountCreditsAggregate,
            AiOrgChargeKind::UntrackableCredits => Self::UntrackableCredits,
            AiOrgChargeKind::Compute => Self::Compute,
            AiOrgChargeKind::Seats => Self::Seats,
            AiOrgChargeKind::Unrecognized => Self::Unrecognized,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryResponse {
    /// The bare repository name; empty when the row names none.
    repository: String,
    product: String,
    sku: String,
    unit: String,
    kind: ChargeKindResponse,
    quantity: String,
    /// The quantity in credits, for credit rows only.
    credits: Option<String>,
    gross_usd: String,
    discount_usd: String,
    net_usd: String,
    days: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodeReviewResponse {
    by_accounts: String,
    by_repositories: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResidualStatusResponse {
    Settled,
    Provisional,
    Incomplete,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResidualGroupResponse {
    sku: String,
    model: String,
    residual: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResidualDayResponse {
    #[serde(with = "iso_date")]
    day: Date,
    org_gross: String,
    accounts_gross: String,
    /// Organization minus accounts: unknown account, or an import gap on an
    /// incomplete day.
    residual: String,
    status: ResidualStatusResponse,
    accounts_exceed_org: bool,
    groups: Vec<ResidualGroupResponse>,
}

#[cfg(test)]
mod tests;
