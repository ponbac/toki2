//! Admin-only monthly billing of AI usage, under `/ai-usage/admin`.
//!
//! Every route needs admin power: an admin in a browser session, never an API
//! token (`auth::require_admin_power`). Responses are at most as fine as one
//! local day per developer, provider, model, machine and project; they never
//! carry hours or sessions (see `domain::models::ai_billing`).
//!
//! Estimates are USD numbers of the priced usage only: while `unpricedRecords`
//! is positive, `apiEquivalentUsd` is a known subtotal, and the rest is unknown.
//! What a line bills (`billableAmount`) is an exact decimal string in its
//! currency: a fee share, or an API estimate rounded to whole cents. Totals add
//! up those amounts per currency; no amount is converted. Completeness reports
//! days, never times of day: a machine's last sync is its local date.

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{FromRef, Path, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue,
    },
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::Serialize;

use crate::{
    adapters::inbound::http::{ai_usage::AiUsageProvider, ProjectResponse},
    auth::require_admin_power,
    domain::{
        models::{
            AiAllocationBasis, AiBilledDayUsage, AiBillingCharge, AiBillingDeveloper,
            AiBillingLine, AiBillingMonth, AiBillingUsage, AiDeveloperCompleteness,
            AiDeveloperMonth, AiDeveloperReadiness, AiMachineCompleteness, AiMappedProject,
            AiMonthBill, AiMonthCompleteness, AiMonthOverview, AiMonthTotals,
            AiProviderCompleteness, AiSubscriptionMonth, AiUsageDateRange, AiUsdNanos, UserId,
            MACHINE_STALE_AFTER,
        },
        ports::inbound::AiBillingService,
        AiBillingError,
    },
    routes::ApiError,
};

/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;

/// How the CSV names usage without a project and fees without usage.
const UNASSIGNED: &str = "Unassigned";
const UNALLOCATED_OVERHEAD: &str = "Unallocated overhead";

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiBillingService>: FromRef<S>,
{
    Router::new()
        .route("/billing/{month}", get(month_overview))
        .route("/billing/{month}/completeness", get(month_completeness))
        .route(
            "/billing/{month}/developers/{user_id}",
            get(developer_month),
        )
        .route("/billing/{month}/export.csv", get(export_csv))
        .route("/developers", get(list_users))
        .route_layer(middleware::from_fn(require_admin_power))
}

impl From<AiBillingError> for ApiError {
    fn from(err: AiBillingError) -> Self {
        match err {
            AiBillingError::UserNotFound => Self::not_found(err.to_string()),
            AiBillingError::NumericRange => Self::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                err.to_string(),
            ),
            AiBillingError::Storage(message) => {
                tracing::error!("AI billing operation failed: {}", message);
                Self::internal("ai billing operation failed")
            }
        }
    }
}

fn parse_month(raw: &str) -> Result<AiBillingMonth, ApiError> {
    AiBillingMonth::parse(raw)
        .ok_or_else(|| ApiError::bad_request(format!("expected a month as YYYY-MM, got {raw:?}")))
}

/// Everyone's bill for a month in the configured time zone.
async fn month_overview(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Json<MonthOverviewResponse>, ApiError> {
    let month = parse_month(&month)?;
    let overview = service.month_overview(month).await?;

    Ok(Json(MonthOverviewResponse {
        period: PeriodResponse::new(month, service.time_zone().as_str()),
        developers: overview.developers.iter().map(Into::into).collect(),
        totals: overview.bill.totals()?.into(),
        lines: overview.bill.lines.iter().map(Into::into).collect(),
        subscriptions: overview.bill.subscriptions.iter().map(Into::into).collect(),
    }))
}

/// One developer's month: their bill and their usage per local day by
/// provider, model, machine and project.
async fn developer_month(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path((month, user_id)), _): PathParam<(String, i32)>,
) -> Result<Json<DeveloperMonthResponse>, ApiError> {
    let month = parse_month(&month)?;
    let AiDeveloperMonth {
        developer,
        bill,
        usage,
        machines,
    } = service.developer_month(month, UserId::new(user_id)).await?;

    Ok(Json(DeveloperMonthResponse {
        period: PeriodResponse::new(month, service.time_zone().as_str()),
        developer: (&developer).into(),
        totals: bill.totals()?.into(),
        lines: bill.lines.iter().map(Into::into).collect(),
        subscriptions: bill.subscriptions.iter().map(Into::into).collect(),
        machines: machines
            .into_iter()
            .map(|machine| MachineLabelResponse {
                machine_id: machine.machine_id.to_string(),
                label: machine.label,
            })
            .collect(),
        daily_usage: usage.into_iter().map(Into::into).collect(),
    }))
}

/// Whether each developer's machines have uploaded all of a month's usage.
async fn month_completeness(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Json<CompletenessResponse>, ApiError> {
    let month = parse_month(&month)?;
    let completeness = service.completeness(month).await?;

    Ok(Json(completeness.into()))
}

/// The month's bill as CSV, one row per project, developer, provider and charge.
async fn export_csv(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Response, ApiError> {
    let month = parse_month(&month)?;
    let overview = service.month_overview(month).await?;
    let disposition = format!("attachment; filename=\"ai-usage-billing-{month}.csv\"");

    Ok((
        [
            (
                CONTENT_TYPE,
                HeaderValue::from_static("text/csv; charset=utf-8"),
            ),
            (
                CONTENT_DISPOSITION,
                HeaderValue::from_str(&disposition)
                    .map_err(|_| ApiError::internal("invalid export file name"))?,
            ),
        ],
        render_csv(&overview),
    )
        .into_response())
}

/// Every user, by name, such as to declare a subscription for.
async fn list_users(
    State(service): State<Arc<dyn AiBillingService>>,
) -> Result<Json<Vec<DeveloperResponse>>, ApiError> {
    let users = service.users().await?;

    Ok(Json(users.iter().map(Into::into).collect()))
}

/// The month and the time zone whose calendar it follows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PeriodResponse {
    /// `YYYY-MM`.
    month: String,
    /// `YYYY-MM-DD`.
    first_day: String,
    /// `YYYY-MM-DD`, inclusive.
    last_day: String,
    time_zone: String,
}

impl PeriodResponse {
    fn new(month: AiBillingMonth, time_zone: &str) -> Self {
        Self {
            month: month.to_string(),
            first_day: month.first_day().to_string(),
            last_day: month.last_day().to_string(),
            time_zone: time_zone.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonthOverviewResponse {
    #[serde(flatten)]
    period: PeriodResponse,
    developers: Vec<DeveloperResponse>,
    lines: Vec<BillingLineResponse>,
    subscriptions: Vec<SubscriptionMonthResponse>,
    totals: TotalsResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperMonthResponse {
    #[serde(flatten)]
    period: PeriodResponse,
    developer: DeveloperResponse,
    lines: Vec<BillingLineResponse>,
    subscriptions: Vec<SubscriptionMonthResponse>,
    totals: TotalsResponse,
    machines: Vec<MachineLabelResponse>,
    /// Usage per local day, provider, model, machine and project key.
    daily_usage: Vec<DayUsageResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperResponse {
    user_id: i32,
    full_name: String,
    email: String,
}

impl From<&AiBillingDeveloper> for DeveloperResponse {
    fn from(developer: &AiBillingDeveloper) -> Self {
        Self {
            user_id: developer.user_id.as_i32(),
            full_name: developer.full_name.clone(),
            email: developer.email.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokensResponse {
    input: i64,
    cache_read: i64,
    cache_write: i64,
    output: i64,
    total: i64,
}

/// Summed usage. `apiEquivalentUsd` is the estimate of the priced records; while
/// `unpricedRecords` is positive, the full cost is unknown.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageResponse {
    api_equivalent_usd: f64,
    unpriced_records: i64,
    records: i64,
    tokens: TokensResponse,
}

impl From<&AiBillingUsage> for UsageResponse {
    fn from(usage: &AiBillingUsage) -> Self {
        Self {
            api_equivalent_usd: usage.priced_cost.to_usd(),
            unpriced_records: usage.unpriced_records,
            records: usage.records,
            tokens: TokensResponse {
                input: usage.tokens.input,
                cache_read: usage.tokens.cache_read,
                cache_write: usage.tokens.cache_write,
                output: usage.tokens.output,
                total: usage.total_tokens(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum BillingModeResponse {
    /// Billed at the estimated API cost, in USD.
    Api,
    /// Billed as a share of a subscription fee.
    Subscription,
}

/// One charge of a developer's usage of a provider to a project.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BillingLineResponse {
    user_id: i32,
    provider: AiUsageProvider,
    /// Null for Unassigned usage and for unallocated overhead.
    project: Option<ProjectResponse>,
    billing_mode: BillingModeResponse,
    /// A subscription fee without usage in the month; it has no project.
    unallocated_overhead: bool,
    subscription_id: Option<i32>,
    /// What the line bills, an exact decimal in `billableCurrency`: the fee
    /// share, or for API usage the estimate rounded to whole cents. Null when
    /// all of an API line's usage is unpriced, so its cost is unknown.
    billable_amount: Option<String>,
    /// The fee's currency, or `USD` for API usage.
    billable_currency: String,
    /// The usage the line charges for, with its unrounded estimate.
    usage: UsageResponse,
}

fn project_response(project: &Option<AiMappedProject>) -> Option<ProjectResponse> {
    project.as_ref().map(|project| ProjectResponse {
        project_id: project.id.to_string(),
        project_name: project.name.clone(),
    })
}

impl From<&AiBillingLine> for BillingLineResponse {
    fn from(line: &AiBillingLine) -> Self {
        let (billing_mode, overhead, subscription_id, currency) = match &line.charge {
            AiBillingCharge::Api => (BillingModeResponse::Api, false, None, "USD".to_string()),
            AiBillingCharge::Subscription {
                subscription_id,
                fee,
            } => (
                BillingModeResponse::Subscription,
                false,
                Some(subscription_id.as_i32()),
                fee.currency.as_str().to_string(),
            ),
            AiBillingCharge::UnallocatedOverhead {
                subscription_id,
                fee,
            } => (
                BillingModeResponse::Subscription,
                true,
                Some(subscription_id.as_i32()),
                fee.currency.as_str().to_string(),
            ),
        };

        Self {
            user_id: line.user_id.as_i32(),
            provider: line.provider.into(),
            project: project_response(&line.project),
            billing_mode,
            unallocated_overhead: overhead,
            subscription_id,
            billable_amount: line.billable().map(|amount| amount.to_decimal()),
            billable_currency: currency,
            usage: (&line.usage).into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum AllocationResponse {
    /// By each project's priced API-equivalent cost.
    ApiCost,
    /// By token share: no covered usage has a priced cost.
    Tokens,
    /// By record share: covered usage has no priced cost or tokens.
    Records,
    /// No usage on the covered days: the fee is overhead.
    Unallocated,
}

/// A subscription's charge for the month.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionMonthResponse {
    subscription_id: i32,
    user_id: i32,
    provider: AiUsageProvider,
    plan: String,
    /// The declared monthly fee, an exact decimal in `currency`.
    monthly_cost: String,
    currency: String,
    valid_from: String,
    valid_to: Option<String>,
    /// The first and last day of the month the subscription covers, inclusive.
    covered_from: String,
    covered_to: String,
    covered_days: i64,
    days_in_month: u8,
    /// `monthlyCost × coveredDays / daysInMonth`, rounded half up.
    prorated_fee: String,
    allocation: AllocationResponse,
    /// Usage on the covered days, over all projects.
    usage: UsageResponse,
}

impl From<&AiSubscriptionMonth> for SubscriptionMonthResponse {
    fn from(month: &AiSubscriptionMonth) -> Self {
        let subscription = &month.subscription;
        let terms = &subscription.terms;
        Self {
            subscription_id: subscription.id.as_i32(),
            user_id: subscription.user_id.as_i32(),
            provider: terms.provider.into(),
            plan: terms.plan.as_str().to_string(),
            monthly_cost: terms.monthly_cost.to_string(),
            currency: terms.currency.as_str().to_string(),
            valid_from: terms.period.valid_from().to_string(),
            valid_to: terms.period.valid_to().map(|day| day.to_string()),
            covered_from: month.covered.start().to_string(),
            covered_to: month.covered.last_day().to_string(),
            covered_days: month.covered_days(),
            days_in_month: month.days_in_month,
            prorated_fee: month.prorated_fee.to_decimal(),
            allocation: match month.basis {
                Some(AiAllocationBasis::ApiCost) => AllocationResponse::ApiCost,
                Some(AiAllocationBasis::Tokens) => AllocationResponse::Tokens,
                Some(AiAllocationBasis::Records) => AllocationResponse::Records,
                None => AllocationResponse::Unallocated,
            },
            usage: (&month.usage).into(),
        }
    }
}

/// Fees billed in one currency.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeeTotalResponse {
    currency: String,
    /// Allocated shares and overhead together, an exact decimal.
    billed: String,
    /// The unallocated overhead within `billed`.
    overhead: String,
}

/// Month totals; amounts in different currencies are never added together.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsResponse {
    fees: Vec<FeeTotalResponse>,
    /// What API lines bill, in USD: the sum of their `billableAmount`s, each
    /// rounded to whole cents. An exact decimal.
    api_billed_usd: String,
    /// Records on API days whose cost is unknown and not in `apiBilledUsd`.
    api_unpriced_records: i64,
    /// All usage, however it bills, with its unrounded API-equivalent estimate.
    usage: UsageResponse,
}

impl From<AiMonthTotals> for TotalsResponse {
    fn from(totals: AiMonthTotals) -> Self {
        Self {
            fees: totals
                .fees
                .iter()
                .map(|total| FeeTotalResponse {
                    currency: total.billed.currency.as_str().to_string(),
                    billed: total.billed.to_decimal(),
                    overhead: total.overhead.to_decimal(),
                })
                .collect(),
            api_billed_usd: totals.api_billed.to_decimal(),
            api_unpriced_records: totals.api_unpriced_records,
            usage: (&totals.usage).into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineLabelResponse {
    machine_id: String,
    label: String,
}

/// A developer's usage on one local day of one provider, model, machine and
/// project key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayUsageResponse {
    /// `YYYY-MM-DD`.
    day: String,
    provider: AiUsageProvider,
    model: String,
    machine_id: String,
    project_key: String,
    /// The project the key maps to now; null is Unassigned.
    project: Option<ProjectResponse>,
    /// The subscription that pays for the day's usage of the provider, or null
    /// when it bills at its estimated API cost.
    subscription_id: Option<i32>,
    usage: UsageResponse,
}

impl From<AiBilledDayUsage> for DayUsageResponse {
    fn from(billed: AiBilledDayUsage) -> Self {
        let usage = billed.usage;
        Self {
            day: usage.day.to_string(),
            provider: usage.provider.into(),
            model: usage.model,
            machine_id: usage.machine_id.to_string(),
            project: project_response(&usage.project),
            project_key: usage.project_key,
            subscription_id: billed.subscription_id.map(|id| id.as_i32()),
            usage: (&usage.usage).into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletenessResponse {
    month: String,
    /// Machines must have uploaded every day through this local day,
    /// `YYYY-MM-DD`: the month's last day or, while it is in progress,
    /// yesterday. Null before any day of the month has passed.
    required_through: Option<String>,
    in_progress: bool,
    stale_after_days: i64,
    developers: Vec<DeveloperCompletenessResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum ReadinessResponse {
    Ready,
    Incomplete,
    NoSync,
    NoActivity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperCompletenessResponse {
    #[serde(flatten)]
    developer: DeveloperResponse,
    readiness: ReadinessResponse,
    has_usage: bool,
    has_subscription: bool,
    machines: Vec<MachineCompletenessResponse>,
}

/// Local days, inclusive, that were not uploaded.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GapResponse {
    from: String,
    to: String,
}

fn gap_responses(gaps: &[AiUsageDateRange]) -> Vec<GapResponse> {
    gaps.iter()
        .map(|gap| GapResponse {
            from: gap.start().to_string(),
            to: gap.last_day().to_string(),
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineCompletenessResponse {
    machine_id: String,
    label: String,
    client_version: String,
    /// The local date of the last sync; never its time.
    last_synced_on: String,
    stale: bool,
    active_in_month: bool,
    complete: bool,
    /// Required days no provider uploaded.
    gaps: Vec<GapResponse>,
    providers: Vec<ProviderCompletenessResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCompletenessResponse {
    provider: AiUsageProvider,
    /// The latest report's coverage status, if the provider was reported.
    status: Option<&'static str>,
    pricing_status: Option<&'static str>,
    /// Billing needs this provider's uploads from the machine for the month.
    expected: bool,
    /// Required days not uploaded for this provider.
    gaps: Vec<GapResponse>,
}

impl From<&AiProviderCompleteness> for ProviderCompletenessResponse {
    fn from(provider: &AiProviderCompleteness) -> Self {
        Self {
            provider: provider.provider.into(),
            status: provider.latest_report.map(|report| report.status.as_str()),
            pricing_status: provider
                .latest_report
                .and_then(|report| report.pricing_status)
                .map(|status| status.as_str()),
            expected: provider.expected,
            gaps: gap_responses(&provider.gaps),
        }
    }
}

impl From<AiMachineCompleteness> for MachineCompletenessResponse {
    fn from(completeness: AiMachineCompleteness) -> Self {
        let machine = completeness.machine;
        Self {
            machine_id: machine.machine_id.to_string(),
            label: machine.label,
            client_version: machine.client_version,
            last_synced_on: machine.last_synced_on.to_string(),
            stale: completeness.stale,
            active_in_month: completeness.active_in_month,
            complete: completeness.complete,
            gaps: gap_responses(&completeness.gaps),
            providers: completeness.providers.iter().map(Into::into).collect(),
        }
    }
}

impl From<AiDeveloperCompleteness> for DeveloperCompletenessResponse {
    fn from(completeness: AiDeveloperCompleteness) -> Self {
        Self {
            developer: (&completeness.developer).into(),
            readiness: match completeness.readiness {
                AiDeveloperReadiness::Ready => ReadinessResponse::Ready,
                AiDeveloperReadiness::Incomplete => ReadinessResponse::Incomplete,
                AiDeveloperReadiness::NoSync => ReadinessResponse::NoSync,
                AiDeveloperReadiness::NoActivity => ReadinessResponse::NoActivity,
            },
            has_usage: completeness.has_usage,
            has_subscription: completeness.has_subscription,
            machines: completeness.machines.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<AiMonthCompleteness> for CompletenessResponse {
    fn from(completeness: AiMonthCompleteness) -> Self {
        Self {
            month: completeness.month.to_string(),
            required_through: completeness.required_through.map(|day| day.to_string()),
            in_progress: completeness.in_progress,
            stale_after_days: MACHINE_STALE_AFTER.whole_days(),
            developers: completeness
                .developers
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

/// The CSV columns, in order.
const CSV_HEADER: [&str; 18] = [
    "month",
    "project_id",
    "project",
    "developer",
    "provider",
    "billing_mode",
    "plan",
    "billable_amount",
    "billable_currency",
    "api_equivalent_usd",
    "unpriced_records",
    "input_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "output_tokens",
    "total_tokens",
    "allocation_basis",
    "warning",
];

/// Renders a month's bill as RFC 4180 CSV with CRLF line ends and a UTF-8 byte
/// order mark, so spreadsheet programs read names with accents correctly.
///
/// Each row is one charge of one developer's use of one provider to one
/// project. `billable_amount` is what to bill, in `billable_currency`, and is
/// the same amount the overview shows:
///
/// - `subscription` rows bill their share of the pro-rated fee, exactly, in the
///   fee's currency. `api_equivalent_usd` is the estimate the share was weighed
///   by. A fee without usage is `Unallocated overhead` with no project.
/// - `api` rows bill their API-equivalent estimate in USD, rounded to whole
///   cents; `api_equivalent_usd` keeps it unrounded.
///
/// Summing `billable_amount` per `billable_currency` gives the overview's
/// totals. Amounts in different currencies are never combined or converted. An
/// estimate that is wholly unknown, because every record is unpriced, is empty
/// rather than zero. Rows are ordered by project (Unassigned, then overhead,
/// last), developer, provider and billing mode. Every cell is quoted and
/// guarded against spreadsheet formula injection (`csv_cell`).
pub fn render_csv(overview: &AiMonthOverview) -> String {
    let bill: &AiMonthBill = &overview.bill;
    let names: HashMap<i32, &str> = overview
        .developers
        .iter()
        .map(|developer| (developer.user_id.as_i32(), developer.full_name.as_str()))
        .collect();
    let subscriptions: HashMap<i32, &AiSubscriptionMonth> = bill
        .subscriptions
        .iter()
        .map(|month| (month.subscription.id.as_i32(), month))
        .collect();

    let mut lines: Vec<&AiBillingLine> = bill.lines.iter().collect();
    let project_rank = |line: &AiBillingLine| match (&line.charge, &line.project) {
        (AiBillingCharge::UnallocatedOverhead { .. }, _) => 2,
        (_, None) => 1,
        (_, Some(_)) => 0,
    };
    let developer = |line: &AiBillingLine| names.get(&line.user_id.as_i32()).copied().unwrap_or("");
    lines.sort_by(|a, b| {
        project_rank(a)
            .cmp(&project_rank(b))
            .then_with(|| {
                let name = |line: &AiBillingLine| {
                    line.project
                        .as_ref()
                        .map(|project| (project.name.clone(), project.id.to_string()))
                };
                name(a).cmp(&name(b))
            })
            .then_with(|| developer(a).cmp(developer(b)))
            .then_with(|| a.user_id.as_i32().cmp(&b.user_id.as_i32()))
            .then_with(|| a.provider.as_str().cmp(b.provider.as_str()))
            .then_with(|| {
                let mode = |line: &AiBillingLine| matches!(line.charge, AiBillingCharge::Api);
                mode(a).cmp(&mode(b))
            })
    });

    let mut csv = String::from("\u{feff}");
    push_row(&mut csv, CSV_HEADER.iter().map(|cell| cell.to_string()));
    for line in lines {
        let subscription = match &line.charge {
            AiBillingCharge::Api => None,
            AiBillingCharge::Subscription {
                subscription_id, ..
            }
            | AiBillingCharge::UnallocatedOverhead {
                subscription_id, ..
            } => subscriptions.get(&subscription_id.as_i32()).copied(),
        };
        let (project_id, project) = match (&line.charge, &line.project) {
            (AiBillingCharge::UnallocatedOverhead { .. }, _) => {
                (String::new(), UNALLOCATED_OVERHEAD.to_string())
            }
            (_, Some(project)) => (project.id.to_string(), project.name.clone()),
            (_, None) => (String::new(), UNASSIGNED.to_string()),
        };
        let usage = &line.usage;
        let unknown_cost = usage.priced_cost == AiUsdNanos::ZERO && usage.unpriced_records > 0;
        let billing_mode = match &line.charge {
            AiBillingCharge::Api => "api",
            _ => "subscription",
        };
        let billable = line.billable();
        let (billable_amount, billable_currency) = match (&line.charge, &billable) {
            (_, Some(amount)) => (amount.to_decimal(), amount.currency.as_str().to_string()),
            (AiBillingCharge::Api, None) => (String::new(), "USD".to_string()),
            (_, None) => (String::new(), String::new()),
        };
        let basis = subscription.and_then(|month| month.basis);

        push_row(
            &mut csv,
            [
                bill.month.to_string(),
                project_id,
                project,
                developer(line).to_string(),
                line.provider.as_str().to_string(),
                billing_mode.to_string(),
                subscription
                    .map(|month| month.subscription.terms.plan.as_str().to_string())
                    .unwrap_or_default(),
                billable_amount,
                billable_currency,
                if unknown_cost {
                    String::new()
                } else {
                    usage.priced_cost.to_decimal(4)
                },
                usage.unpriced_records.to_string(),
                usage.tokens.input.to_string(),
                usage.tokens.cache_read.to_string(),
                usage.tokens.cache_write.to_string(),
                usage.tokens.output.to_string(),
                usage.total_tokens().to_string(),
                match (&line.charge, basis) {
                    (AiBillingCharge::Api, _) => "",
                    (_, Some(AiAllocationBasis::ApiCost)) => "api_cost",
                    (_, Some(AiAllocationBasis::Tokens)) => "tokens",
                    (_, Some(AiAllocationBasis::Records)) => "records",
                    (_, None) => "none",
                }
                .to_string(),
                warning(line, basis),
            ],
        );
    }

    csv
}

/// Why a row's figures need a second look, if they do.
fn warning(line: &AiBillingLine, basis: Option<AiAllocationBasis>) -> String {
    let unpriced = line.usage.unpriced_records;
    let mut warnings = Vec::new();
    match (&line.charge, basis) {
        (AiBillingCharge::UnallocatedOverhead { .. }, _) => {
            warnings.push("no usage on the days the subscription covers".to_string());
        }
        (AiBillingCharge::Api, _) if unpriced > 0 => warnings.push(format!(
            "{unpriced} unpriced records: their cost is unknown and not billed"
        )),
        (_, Some(AiAllocationBasis::Tokens)) => {
            warnings.push("split by token share: no covered usage has a priced cost".to_string());
        }
        (_, Some(AiAllocationBasis::Records)) => warnings
            .push("split by record share: covered usage has no priced cost or tokens".to_string()),
        _ => {}
    }
    if unpriced > 0 && matches!(basis, Some(AiAllocationBasis::ApiCost)) {
        warnings.push(format!(
            "{unpriced} unpriced records: they carry no weight in the split"
        ));
    }

    warnings.join("; ")
}

fn push_row(csv: &mut String, cells: impl IntoIterator<Item = String>) {
    let cells: Vec<String> = cells.into_iter().map(|cell| csv_cell(&cell)).collect();
    csv.push_str(&cells.join(","));
    csv.push_str("\r\n");
}

/// Characters that make a spreadsheet read the text after them as a formula.
const FORMULA_PREFIXES: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];
/// Characters a spreadsheet may split a cell on, whatever the file's separator:
/// Swedish-locale Excel splits on `;`.
const SEPARATORS: [char; 5] = [',', ';', '\t', '\r', '\n'];

/// One CSV cell: always quoted, with quotes doubled, so no separator or line
/// break inside a value splits it. A formula prefix at the start of the value,
/// or right after a separator inside it, gets an apostrophe in front, so even
/// a program that splits the value stays on text. Amounts and counts never
/// start with one.
fn csv_cell(value: &str) -> String {
    let mut guarded = String::with_capacity(value.len() + 3);
    let mut at_start = true;
    for character in value.chars() {
        if at_start && FORMULA_PREFIXES.contains(&character) {
            guarded.push('\'');
        }
        guarded.push(character);
        at_start = SEPARATORS.contains(&character);
    }

    format!("\"{}\"", guarded.replace('"', "\"\""))
}

#[cfg(test)]
mod tests;
