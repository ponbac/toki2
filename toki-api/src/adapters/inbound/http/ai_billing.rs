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
//! up those amounts per currency. Each amount is also converted to the billing
//! currency (`billingCurrency`, SEK) at the month's exchange rate
//! (`convertedAmount`), and converted totals add up the converted lines; see
//! `domain::models::ai_exchange_rate`. Completeness reports days, never times
//! of day: a machine's last sync is its local date, and a rate's fetch its
//! local date.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue,
    },
    middleware,
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};

use crate::{
    adapters::inbound::http::{ai_usage::AiUsageProvider, ProjectResponse},
    auth::{require_admin_power, AuthUser},
    domain::{
        models::{
            AiAllocationBasis, AiBilledDayUsage, AiBillingCharge, AiBillingDeveloper,
            AiBillingLine, AiBillingMonth, AiBillingUsage, AiConvertedAmount, AiConvertedBill,
            AiConvertedTotals, AiCurrency, AiDeveloperCompleteness, AiDeveloperMonth,
            AiDeveloperReadiness, AiExchangeRate, AiExchangeRateValue, AiFeeAmount,
            AiMachineCompleteness, AiMappedProject, AiMonthBill, AiMonthCompleteness,
            AiMonthTotals, AiProviderCompleteness, AiSubscriptionMonth, AiUsageDateRange, UserId,
            BILLING_CURRENCY, MACHINE_STALE_AFTER,
        },
        ports::inbound::AiBillingService,
        AiBillingError,
    },
    routes::ApiError,
};

/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;
/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;

mod csv;

pub use csv::render_csv;

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
        .route(
            "/billing/{month}/exchange-rates/{currency}",
            put(override_exchange_rate).delete(reset_exchange_rate),
        )
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
            AiBillingError::Invalid(message) => Self::bad_request(message),
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
    let bill = BillResponse::new(&overview.bill, &overview.converted)?;

    Ok(Json(MonthOverviewResponse {
        period: PeriodResponse::new(month, service.time_zone().as_str()),
        developers: overview.developers.iter().map(Into::into).collect(),
        bill,
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
        converted,
        usage,
        machines,
    } = service.developer_month(month, UserId::new(user_id)).await?;

    Ok(Json(DeveloperMonthResponse {
        period: PeriodResponse::new(month, service.time_zone().as_str()),
        developer: (&developer).into(),
        bill: BillResponse::new(&bill, &converted)?,
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

fn parse_currency(raw: &str) -> Result<AiCurrency, ApiError> {
    AiCurrency::parse(raw).map_err(|_| {
        ApiError::bad_request(format!(
            "expected a three-letter currency code, got {raw:?}"
        ))
    })
}

/// An admin's exchange rate for a month and currency.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExchangeRateOverrideRequest {
    /// Units of the billing currency per unit of the currency, as a decimal
    /// string with at most six decimals, such as `"10.25"`.
    rate: String,
}

/// Overrides a month's exchange rate. A fetched rate never replaces it.
async fn override_exchange_rate(
    State(service): State<Arc<dyn AiBillingService>>,
    user: AuthUser,
    WithRejection(Path((month, currency)), _): PathParam<(String, String)>,
    WithRejection(Json(body), _): JsonBody<ExchangeRateOverrideRequest>,
) -> Result<Json<ExchangeRateResponse>, ApiError> {
    let month = parse_month(&month)?;
    let currency = parse_currency(&currency)?;
    let rate = AiExchangeRateValue::parse(body.rate.trim()).ok_or_else(|| {
        ApiError::bad_request(format!(
            "expected a positive rate with at most six decimals, got {:?}",
            body.rate
        ))
    })?;
    let rate = service
        .override_exchange_rate(month, &currency, rate, &user.id)
        .await?;

    Ok(Json((&rate).into()))
}

/// What a reset leaves: the fetched rate, or null when none can be fetched.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeRateResetResponse {
    exchange_rate: Option<ExchangeRateResponse>,
}

/// Removes an override and fetches the provider's rate again.
async fn reset_exchange_rate(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path((month, currency)), _): PathParam<(String, String)>,
) -> Result<Json<ExchangeRateResetResponse>, ApiError> {
    let month = parse_month(&month)?;
    let currency = parse_currency(&currency)?;
    let rate = service.reset_exchange_rate(month, &currency).await?;

    Ok(Json(ExchangeRateResetResponse {
        exchange_rate: rate.as_ref().map(Into::into),
    }))
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

/// A bill in its original currencies and in the billing currency.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BillResponse {
    /// The currency every amount is converted to, `SEK`.
    billing_currency: &'static str,
    lines: Vec<BillingLineResponse>,
    subscriptions: Vec<SubscriptionMonthResponse>,
    totals: TotalsResponse,
    /// Every rate stored for the month, by currency: the ones the bill uses
    /// and any other override.
    exchange_rates: Vec<ExchangeRateResponse>,
    /// Currencies the bill has non-zero amounts in but no rate for. Their
    /// lines have no converted amount, and converted totals that include them
    /// are null.
    missing_rates: Vec<String>,
    /// Currencies needed by the bill whose fetch is still in progress,
    /// including provisional rates being refreshed. A later request sees the
    /// fetched replacement once it arrives.
    pending_rates: Vec<String>,
}

impl BillResponse {
    fn new(bill: &AiMonthBill, converted: &AiConvertedBill) -> Result<Self, AiBillingError> {
        Ok(Self {
            billing_currency: BILLING_CURRENCY,
            lines: bill
                .lines
                .iter()
                .zip(&converted.lines)
                .map(|(line, converted)| BillingLineResponse::new(line, converted))
                .collect(),
            subscriptions: bill
                .subscriptions
                .iter()
                .zip(&converted.subscriptions)
                .map(|(month, converted)| SubscriptionMonthResponse::new(month, converted))
                .collect(),
            totals: TotalsResponse::new(bill.totals()?, &converted.totals),
            exchange_rates: converted.rates.iter().map(Into::into).collect(),
            missing_rates: codes(&converted.missing_rates),
            pending_rates: codes(&converted.pending_rates),
        })
    }
}

fn codes(currencies: &[AiCurrency]) -> Vec<String> {
    currencies
        .iter()
        .map(|currency| currency.as_str().to_string())
        .collect()
}

/// A month's exchange rate: the rate billed at, and the fetched rate and an
/// admin's override it comes from. Dates only: never when in the day.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExchangeRateResponse {
    /// `YYYY-MM`.
    month: String,
    currency: String,
    /// What the month bills at: units of the billing currency per unit of
    /// `currency`, an exact decimal. The override's rate, else the fetched one.
    rate: String,
    /// `admin` for an override, else the provider's name, such as `riksbank`.
    source: String,
    /// The rate billed at is a fetched average of the days published so far.
    provisional: bool,
    /// The provider's rate, kept beneath an override too.
    fetched: Option<FetchedRateResponse>,
    #[serde(rename = "override")]
    overridden: Option<RateOverrideResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchedRateResponse {
    rate: String,
    /// The provider's name, such as `riksbank`.
    source: String,
    provisional: bool,
    /// The latest day whose daily rate the average includes, `YYYY-MM-DD`.
    observed_through: String,
    /// The local date it was fetched on, `YYYY-MM-DD`.
    fetched_on: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RateOverrideResponse {
    rate: String,
    /// The local date it was set on, `YYYY-MM-DD`.
    overridden_on: String,
    /// The admin who set it, if they still exist.
    overridden_by: Option<String>,
}

impl From<&AiExchangeRate> for ExchangeRateResponse {
    fn from(rate: &AiExchangeRate) -> Self {
        Self {
            month: rate.month.to_string(),
            currency: rate.currency.as_str().to_string(),
            rate: rate
                .rate()
                .map(AiExchangeRateValue::to_decimal)
                .unwrap_or_default(),
            source: rate.source().to_string(),
            provisional: rate.is_provisional(),
            fetched: rate.fetched.as_ref().map(|fetched| FetchedRateResponse {
                rate: fetched.rate.to_decimal(),
                source: fetched.provider.clone(),
                provisional: fetched.provisional,
                observed_through: fetched.observed_through.to_string(),
                fetched_on: fetched.fetched_on.to_string(),
            }),
            overridden: rate
                .overridden
                .as_ref()
                .map(|overridden| RateOverrideResponse {
                    rate: overridden.rate.to_decimal(),
                    overridden_on: overridden.set_on.to_string(),
                    overridden_by: overridden.by.clone(),
                }),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonthOverviewResponse {
    #[serde(flatten)]
    period: PeriodResponse,
    developers: Vec<DeveloperResponse>,
    #[serde(flatten)]
    bill: BillResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeveloperMonthResponse {
    #[serde(flatten)]
    period: PeriodResponse,
    developer: DeveloperResponse,
    #[serde(flatten)]
    bill: BillResponse,
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
    /// What the line bills in the billing currency, an exact decimal:
    /// `billableAmount × exchangeRate` rounded half up to hundredths, or for a
    /// subscription its share of the converted pro-rated fee. Null when the
    /// amount is unknown or its currency has no rate (`missingRate`).
    converted_amount: Option<String>,
    /// The rate `convertedAmount` used; null in the billing currency.
    exchange_rate: Option<String>,
    /// No rate for `billableCurrency`: the converted amount is unknown.
    missing_rate: bool,
    /// The usage the line charges for, with its unrounded estimate.
    usage: UsageResponse,
}

fn project_response(project: &Option<AiMappedProject>) -> Option<ProjectResponse> {
    project.as_ref().map(|project| ProjectResponse {
        project_id: project.id.to_string(),
        project_name: project.name.clone(),
    })
}

impl BillingLineResponse {
    fn new(line: &AiBillingLine, converted: &AiConvertedAmount) -> Self {
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
            converted_amount: converted.amount().map(AiFeeAmount::to_decimal),
            exchange_rate: converted
                .rate()
                .and_then(AiExchangeRate::rate)
                .map(AiExchangeRateValue::to_decimal),
            missing_rate: matches!(converted, AiConvertedAmount::MissingRate { .. }),
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
    /// `proratedFee` in the billing currency, converted once and then split
    /// like the fee; null without a rate.
    converted_prorated_fee: Option<String>,
    /// The rate it was converted at; null in the billing currency.
    exchange_rate: Option<String>,
    allocation: AllocationResponse,
    /// Usage on the covered days, over all projects.
    usage: UsageResponse,
}

impl SubscriptionMonthResponse {
    fn new(month: &AiSubscriptionMonth, converted: &AiConvertedAmount) -> Self {
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
            converted_prorated_fee: converted.amount().map(AiFeeAmount::to_decimal),
            exchange_rate: converted
                .rate()
                .and_then(AiExchangeRate::rate)
                .map(AiExchangeRateValue::to_decimal),
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
    /// Totals in the billing currency: sums of the converted lines.
    converted: ConvertedTotalsResponse,
}

/// Totals in the billing currency, exact decimals. Each is null when a line
/// it includes has no exchange rate.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertedTotalsResponse {
    currency: &'static str,
    /// Everything billed.
    billed: Option<String>,
    /// Subscription fees, overhead included.
    fees: Option<String>,
    /// The unallocated overhead within `fees`.
    overhead: Option<String>,
    /// API usage.
    api: Option<String>,
}

impl TotalsResponse {
    fn new(totals: AiMonthTotals, converted: &AiConvertedTotals) -> Self {
        let decimal = |amount: &Option<AiFeeAmount>| amount.as_ref().map(AiFeeAmount::to_decimal);
        Self {
            converted: ConvertedTotalsResponse {
                currency: BILLING_CURRENCY,
                billed: decimal(&converted.billed),
                fees: decimal(&converted.fees),
                overhead: decimal(&converted.overhead),
                api: decimal(&converted.api),
            },
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

#[cfg(test)]
mod tests;
