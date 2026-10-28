//! Month close over HTTP: the close routes, closed months served from their
//! snapshots, adjustment lines, and the projection a close stores.
//!
//! A snapshot stores what the overview, the developer page and the CSV
//! served at close (`HttpBillProjector`): served again, a closed month has
//! the same JSON values (JSONB may reorder keys) and the same CSV bytes. Its
//! `projection_version` is `AI_BILL_PROJECTION_VERSION` when it was stored;
//! a change to the served JSON bumps the version and adds an upgrade step
//! from the previous one (`upgrade_snapshot`), so the strict frontend
//! contracts keep reading old closes.

use std::collections::HashSet;

use axum::http::StatusCode;
use serde_json::{json, Map};
use time::OffsetDateTime;

use super::*;
use crate::domain::{
    models::{
        convert_bill, AiCheckOutcome, AiCloseCheck, AiCloseCheckId, AiCloseCheckStatus,
        AiCloseProblem, AiCloseReadiness, AiCloseWaiver, AiClosedDeveloperBill, AiClosedMonth,
        AiMeteredSource, AiMonthClose, AiMonthCloseState, AiMonthCloseStatus, AiStoredSnapshotLine,
        AiWaiverReason, AI_BILL_PROJECTION_VERSION,
    },
    ports::inbound::{AiBillProjector, AiProjectedBill, AiProjectionInput},
    AiMonthCloseError,
};

/// A closed month the snapshot of which this code cannot serve: stored by a
/// newer version.
fn version_error(version: i16) -> ApiError {
    tracing::error!(
        stored = version,
        supported = AI_BILL_PROJECTION_VERSION,
        "A month close was stored by a newer projection"
    );
    ApiError::internal("this month's snapshot was stored by a newer version of Toki")
}

/// Brings a stored response (an overview or a developer month assembled from
/// a snapshot) to the current projection. Version 1 is the first: it has no
/// steps. A later version adds `n − 1 → n` here, with a test.
fn upgrade_snapshot(version: i16, response: Value) -> Result<Value, ApiError> {
    match version {
        AI_BILL_PROJECTION_VERSION => Ok(response),
        newer if newer > AI_BILL_PROJECTION_VERSION => Err(version_error(newer)),
        older => {
            tracing::error!(
                stored = older,
                "A month close has an unknown projection version"
            );
            Err(ApiError::internal(
                "this month's snapshot has an unknown version",
            ))
        }
    }
}

/// A closed month whose changes post on an open month, and its close.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AdjustedMonthResponse {
    /// `YYYY-MM`.
    month: String,
    snapshot_id: i64,
}

impl From<&AiAdjustedMonth> for AdjustedMonthResponse {
    fn from(adjusted: &AiAdjustedMonth) -> Self {
        Self {
            month: adjusted.month.to_string(),
            snapshot_id: adjusted.close_id.as_i64(),
        }
    }
}

/// What an adjustment line corrects.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum AdjustedKindResponse {
    Api,
    Subscription,
    Overhead,
    Metered,
}

/// A change to a closed month since it closed, for one subject, project,
/// kind, currency and payer, posted on an open month. Amounts are signed
/// exact decimals: a credit is negative.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AdjustmentLineResponse {
    /// The closed month it corrects, `YYYY-MM`.
    adjusts_month: String,
    /// That month's close.
    snapshot_id: i64,
    subject: SubjectResponse,
    /// The provider of what changed; for a subscription's lines, the
    /// subscription's provider.
    provider: AiUsageProvider,
    /// Null for Unassigned usage, overhead and Spinit's metered lines.
    project: Option<ProjectResponse>,
    unassigned: Option<UnassignedKindResponse>,
    kind: AdjustedKindResponse,
    /// Non-null exactly when `kind` is `metered`.
    metered_kind: Option<MeteredKindResponse>,
    /// Non-null exactly for subscription and overhead lines.
    subscription_id: Option<i32>,
    /// The subscription's plan, while it exists.
    plan: Option<String>,
    /// What changed, in `billableCurrency`.
    billable_amount: String,
    billable_currency: String,
    /// What changed in the billing currency: the difference of the amounts
    /// invoiced in it, at the corrected month's rate. Null when unknown
    /// (`missingRate`).
    converted_amount: Option<String>,
    /// The corrected month's rate; null in the billing currency.
    exchange_rate: Option<String>,
    missing_rate: bool,
    /// Metered kinds' credits, signed; null otherwise.
    credits: Option<String>,
    /// Only the amount in the billing currency changed: the corrected
    /// month's exchange rate did (`rateChanged`), or its amounts in the
    /// billing currency round differently at the same rate.
    sek_only: bool,
    /// The corrected month's rate differs from the one this subject,
    /// project, kind and payer was last invoiced at. False without a rate.
    rate_changed: bool,
}

impl AdjustmentLineResponse {
    pub(super) fn new(line: &AiAdjustmentLine) -> Self {
        let (kind, metered) = match line.key.kind {
            AiAdjustedKind::Api => (AdjustedKindResponse::Api, None),
            AiAdjustedKind::Subscription => (AdjustedKindResponse::Subscription, None),
            AiAdjustedKind::Overhead => (AdjustedKindResponse::Overhead, None),
            AiAdjustedKind::Metered(kind) => (AdjustedKindResponse::Metered, Some(kind.into())),
        };
        Self {
            adjusts_month: line.adjusts.month.to_string(),
            snapshot_id: line.adjusts.close_id.as_i64(),
            subject: (&line.subject).into(),
            provider: line.provider.into(),
            project: project_response(&line.project),
            unassigned: line.unassigned.map(Into::into),
            kind,
            metered_kind: metered,
            subscription_id: line.key.subscription_id().map(|id| id.as_i32()),
            plan: line.plan.clone(),
            billable_amount: line.amount.to_decimal(),
            billable_currency: line.amount.currency.as_str().to_string(),
            converted_amount: line.converted.amount().map(AiFeeAmount::to_decimal),
            exchange_rate: line
                .converted
                .rate()
                .and_then(AiExchangeRate::rate)
                .map(AiExchangeRateValue::to_decimal),
            missing_rate: matches!(line.converted, AiConvertedAmount::MissingRate { .. }),
            credits: line.credits.map(credits_decimal),
            sek_only: line.sek_only(),
            rate_changed: line.rate_changed,
        }
    }
}

/// A closed month's close, on its overview and developer pages.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OverviewCloseResponse {
    close_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    closed_at: OffsetDateTime,
    closed_by: Option<String>,
    /// Where later changes to the month post, `YYYY-MM`.
    adjustments_post_on: String,
    /// The month's rates as stored now. `exchangeRates` keeps the ones it
    /// closed with; an override after close changes these and posts the SEK
    /// difference as an adjustment.
    current_exchange_rates: Vec<ExchangeRateResponse>,
}

impl OverviewCloseResponse {
    fn new(
        close: &AiMonthClose,
        adjustments_post_on: AiBillingMonth,
        current_rates: &[AiExchangeRate],
    ) -> Self {
        Self {
            close_id: close.id.as_i64(),
            closed_at: close.closed_at,
            closed_by: close.closed_by.clone(),
            adjustments_post_on: adjustments_post_on.to_string(),
            current_exchange_rates: current_rates.iter().map(Into::into).collect(),
        }
    }
}

fn to_value(value: impl Serialize) -> Result<Value, AiBillingError> {
    serde_json::to_value(value)
        .map_err(|error| AiBillingError::Storage(format!("a bill cannot be projected: {error}")))
}

fn object(value: Value) -> Result<Map<String, Value>, ApiError> {
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(ApiError::internal("a stored snapshot is not an object")),
    }
}

/// A closed month's overview: its stored bill, lines and adjustment lines,
/// and its close.
pub(super) fn closed_overview(closed: &AiClosedMonth) -> Result<Value, ApiError> {
    let snapshot = &closed.snapshot;
    let mut response = object(snapshot.bill_json.clone())?;
    let lines = |adjustments: bool| -> Vec<Value> {
        snapshot
            .lines
            .iter()
            .filter(|line| line.adjusts.is_some() == adjustments)
            .map(|line| line.line_json.clone())
            .collect()
    };
    response.insert("lines".to_string(), Value::Array(lines(false)));
    response.insert("adjustmentLines".to_string(), Value::Array(lines(true)));
    response.insert(
        "close".to_string(),
        to_value(OverviewCloseResponse::new(
            &closed.close,
            closed.adjustments_post_on,
            &closed.current_rates,
        ))?,
    );
    upgrade_snapshot(snapshot.projection_version, Value::Object(response))
}

/// What the developer page shows live in a closed month too.
pub(super) struct LiveDeveloper {
    pub developer: DeveloperResponse,
    pub machines: Vec<MachineLabelResponse>,
    pub daily_usage: Vec<DayUsageResponse>,
}

/// The overview fields a closed developer page takes from the snapshot.
const DEVELOPER_BILL_FIELDS: [&str; 7] = [
    "month",
    "firstDay",
    "lastDay",
    "timeZone",
    "meteredPeriod",
    "billingCurrency",
    "exchangeRates",
];

/// The developer page's own bill part, as stored per developer: its missing
/// and pending rates are the developer's own currencies', as the live page
/// converts only the developer's bill; the corrected months' pending rates
/// are all of them, as on the live page.
const DEVELOPER_PART_FIELDS: [&str; 6] = [
    "subscriptions",
    "totals",
    "metered",
    "missingRates",
    "pendingRates",
    "adjustedPendingRates",
];

/// A closed month's developer page: the overview's period and rates, the
/// developer's stored bill part and lines, their entry of the overview's
/// personal usage (null without one), and the live usage by day.
pub(super) fn closed_developer_month(
    bill: AiClosedDeveloperBill,
    cutover: Option<AiMeteredCutover>,
    live: LiveDeveloper,
) -> Result<Value, ApiError> {
    let AiClosedDeveloperBill {
        close,
        adjustments_post_on,
        current_rates,
        bill_json,
        projection_version,
        developer,
        lines,
        adjustment_lines,
    } = bill;
    let stored = object(bill_json)?;
    let mut response = Map::new();
    for field in DEVELOPER_BILL_FIELDS {
        response.insert(
            field.to_string(),
            stored.get(field).cloned().unwrap_or(Value::Null),
        );
    }
    let part = match developer {
        Some(part) => object(part)?,
        None => empty_part(close.month, &stored, cutover)?,
    };
    for field in DEVELOPER_PART_FIELDS {
        response.insert(
            field.to_string(),
            part.get(field).cloned().unwrap_or(Value::Null),
        );
    }
    response.insert("lines".to_string(), Value::Array(lines));
    response.insert(
        "adjustmentLines".to_string(),
        Value::Array(adjustment_lines),
    );
    response.insert("developer".to_string(), to_value(&live.developer)?);
    response.insert("machines".to_string(), to_value(&live.machines)?);
    response.insert("dailyUsage".to_string(), to_value(&live.daily_usage)?);
    response.insert(
        "personalUsage".to_string(),
        stored_personal_usage(&stored, live.developer.user_id),
    );
    response.insert(
        "close".to_string(),
        to_value(OverviewCloseResponse::new(
            &close,
            adjustments_post_on,
            &current_rates,
        ))?,
    );
    upgrade_snapshot(projection_version, Value::Object(response))
}

/// A developer's personal usage as the closed month's overview serves it:
/// the usage of their entry of `personalUsage`, or null without one.
fn stored_personal_usage(stored: &Map<String, Value>, user_id: i32) -> Value {
    stored
        .get("personalUsage")
        .and_then(Value::as_array)
        .and_then(|entries| entries.iter().find(|entry| entry["userId"] == user_id))
        .and_then(|entry| entry.get("usage").cloned())
        .unwrap_or(Value::Null)
}

/// The bill part of a developer with nothing in a closed month: no
/// subscriptions, zero totals, and in a metered month the payer's empty
/// part, as the live page shows it.
fn empty_part(
    month: AiBillingMonth,
    stored: &Map<String, Value>,
    cutover: Option<AiMeteredCutover>,
) -> Result<Map<String, Value>, ApiError> {
    let metered = stored
        .get("metered")
        .is_some_and(|metered| !metered.is_null())
        .then(|| {
            cutover.map(|cutover| AiMeteredBillInfo {
                payer: cutover.payer,
                provisional: false,
                days_not_imported: 0,
                low_ratio_groups: 0,
                unbilled: Default::default(),
            })
        })
        .flatten();
    let bill = AiMonthBill {
        month,
        lines: Vec::new(),
        subscriptions: Vec::new(),
        metered,
        adjustments: Vec::new(),
    };
    let converted = convert_bill(&bill, &[], &[])?;
    object(to_value(BillResponse::new(&bill, &converted)?)?)
}

/// A closed month's CSV: its stored header and rows, guarded as when it
/// closed, in the stored order.
pub(super) fn closed_csv(closed: &AiClosedMonth) -> Result<String, ApiError> {
    let snapshot = &closed.snapshot;
    if snapshot.projection_version > AI_BILL_PROJECTION_VERSION {
        return Err(version_error(snapshot.projection_version));
    }
    let mut rows: Vec<&AiStoredSnapshotLine> = snapshot.lines.iter().collect();
    rows.sort_by_key(|line| line.csv_position);
    Ok(csv::render(
        &snapshot.csv_header,
        rows.into_iter().map(|line| &line.csv_row),
    ))
}

/// The projection a close stores: what this adapter serves.
pub(super) struct HttpBillProjector {
    pub time_zone: String,
    pub cutover: Option<AiMeteredCutover>,
}

impl AiBillProjector for HttpBillProjector {
    fn project(&self, input: &AiProjectionInput<'_>) -> Result<AiProjectedBill, AiBillingError> {
        let overview =
            MonthOverviewResponse::new(input.overview, input.parts, &self.time_zone, self.cutover)?;
        let Value::Object(mut bill) = to_value(&overview)? else {
            return Err(AiBillingError::Storage(
                "an overview is not an object".to_string(),
            ));
        };
        let mut take = |key: &str| match bill.remove(key) {
            Some(Value::Array(values)) => values,
            _ => Vec::new(),
        };
        let lines = take("lines");
        let adjustment_lines = take("adjustmentLines");
        bill.remove("close");

        let mut developers = Map::new();
        for (user_id, part, converted) in input.developer_bills {
            let Value::Object(mut response) = to_value(BillResponse::new(part, converted)?)? else {
                continue;
            };
            let kept: Map<String, Value> = DEVELOPER_PART_FIELDS
                .iter()
                .filter_map(|field| Some((field.to_string(), response.remove(*field)?)))
                .collect();
            developers.insert(user_id.as_i32().to_string(), Value::Object(kept));
        }

        let table = csv::table(
            input.overview,
            input.parts,
            &self.time_zone,
            Some(input.close_id),
        )?;
        Ok(AiProjectedBill {
            version: AI_BILL_PROJECTION_VERSION,
            bill_json: Value::Object(bill),
            developer_json: Value::Object(developers),
            lines,
            adjustment_lines,
            csv_header: table.header,
            csv_rows: table.rows,
            csv_order: table.order,
        })
    }

    fn project_readiness(&self, readiness: &AiCloseReadiness) -> Value {
        serde_json::to_value(CloseReadinessResponse::from(readiness)).unwrap_or(Value::Null)
    }
}

/// How an account's or the organization's latest check fails.
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum CheckOutcomeResponse {
    Missing,
    /// The import's own error.
    Failed {
        error: String,
    },
    /// A run stopped, or died, before finishing the month; `error` says why.
    Interrupted {
        error: String,
    },
    Differs,
    BeforeSettled {
        #[serde(with = "time::serde::rfc3339")]
        checked_at: OffsetDateTime,
    },
}

impl From<&AiCheckOutcome> for CheckOutcomeResponse {
    fn from(outcome: &AiCheckOutcome) -> Self {
        match outcome {
            AiCheckOutcome::Missing => Self::Missing,
            AiCheckOutcome::Failed(error) => Self::Failed {
                error: error.clone(),
            },
            AiCheckOutcome::Interrupted(error) => Self::Interrupted {
                error: error.clone(),
            },
            AiCheckOutcome::Differs => Self::Differs,
            AiCheckOutcome::BeforeSettled { checked_at } => Self::BeforeSettled {
                checked_at: *checked_at,
            },
        }
    }
}

/// An expected provider's days not uploaded.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderGapsResponse {
    provider: AiUsageProvider,
    gaps: Vec<GapResponse>,
}

/// What a failed check found. Credits are signed hundredths of a credit as
/// exact decimals (`"5.00"`), instants RFC 3339, days `YYYY-MM-DD`.
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ProblemResponse {
    /// A live import run, started at `startedAt`, is importing the month.
    /// No waiver covers it: its check stays failed until the run finishes.
    ImportRunning {
        #[serde(with = "time::serde::rfc3339")]
        started_at: OffsetDateTime,
    },
    /// The payer may still change the month's last days, until `settlesOn`.
    NotSettled {
        last_day: String,
        settles_on: String,
        settle_days: u32,
    },
    /// Days before the import's backfill.
    DaysBeforeBackfill {
        first_day: String,
        last_day: String,
    },
    /// Days whose slices of a source are missing or not yet complete.
    DaysNotImported {
        /// `accountCredits`, `orgCredits` or `orgUsage`.
        source: &'static str,
        days: u32,
        first_day: String,
        last_day: String,
    },
    AccountCheck {
        account_id: i64,
        login: String,
        outcome: CheckOutcomeResponse,
    },
    OrgCheck {
        outcome: CheckOutcomeResponse,
    },
    UnlinkedCredits {
        account_id: i64,
        login: String,
        credits: String,
    },
    ResidualNotSettled {
        reasons: Vec<ProvisionalReasonResponse>,
    },
    /// A day's residual, signed: positive for accounts Toki never listed.
    Residual {
        day: String,
        credits: String,
    },
    UnmappedRepository {
        repository: String,
        credits: String,
    },
    UnrecognizedRepositoryRow {},
    MeteredPricing {
        account_id: i64,
        login: String,
        reasons: Vec<ProvisionalReasonResponse>,
    },
    OldBasis {
        user_id: i32,
        full_name: String,
        machine_id: String,
        label: String,
        provider: AiUsageProvider,
        buckets: i64,
        first_day: String,
        last_day: String,
        /// Their estimate, an exact decimal of USD.
        estimate_usd: String,
    },
    RateMissing {
        currency: String,
    },
    RatePending {
        currency: String,
    },
    RateProvisional {
        currency: String,
        observed_through: String,
    },
    AdjustedRateMissing {
        month: String,
        currency: String,
    },
    AdjustedRatePending {
        month: String,
        currency: String,
    },
    AdjustedRateProvisional {
        month: String,
        currency: String,
    },
    MachineIncomplete {
        user_id: i32,
        full_name: String,
        machine_id: String,
        label: String,
        gaps: Vec<GapResponse>,
        providers: Vec<ProviderGapsResponse>,
    },
    UtcHoursNotUploaded {
        user_id: i32,
        full_name: String,
        machine_id: String,
        label: String,
        provider: Option<AiUsageProvider>,
        #[serde(with = "time::serde::rfc3339")]
        from: OffsetDateTime,
        #[serde(with = "time::serde::rfc3339")]
        until: OffsetDateTime,
    },
}

fn source_name(source: AiMeteredSource) -> &'static str {
    match source {
        AiMeteredSource::AccountCredits => "accountCredits",
        AiMeteredSource::OrgCredits => "orgCredits",
        AiMeteredSource::OrgUsage => "orgUsage",
        AiMeteredSource::Summary => "summary",
    }
}

/// Nanodollars as signed hundredths of a credit, rounded half away from
/// zero: a credit is 10⁷ nanodollars.
fn nanos_as_credits(nanos: i64) -> String {
    const NANOS_PER_HUNDREDTH: i64 = 100_000;
    let magnitude = (nanos.unsigned_abs() + NANOS_PER_HUNDREDTH.unsigned_abs() / 2)
        / NANOS_PER_HUNDREDTH.unsigned_abs();
    let hundredths = i64::try_from(magnitude).unwrap_or(i64::MAX);
    AiFeeAmount {
        hundredths: if nanos < 0 { -hundredths } else { hundredths },
        currency: AiCurrency::usd(),
    }
    .to_decimal()
}

impl From<&AiCloseProblem> for ProblemResponse {
    fn from(problem: &AiCloseProblem) -> Self {
        let reasons = |reasons: &[AiMeteredProvisional]| -> Vec<ProvisionalReasonResponse> {
            reasons.iter().map(|&reason| reason.into()).collect()
        };
        match problem {
            AiCloseProblem::ImportRunning { started_at } => Self::ImportRunning {
                started_at: *started_at,
            },
            AiCloseProblem::NotSettled {
                last_day,
                settles_on,
                settle_days,
            } => Self::NotSettled {
                last_day: last_day.to_string(),
                settles_on: settles_on.to_string(),
                settle_days: *settle_days,
            },
            AiCloseProblem::DaysBeforeBackfill { first, last } => Self::DaysBeforeBackfill {
                first_day: first.to_string(),
                last_day: last.to_string(),
            },
            AiCloseProblem::DaysNotImported {
                source,
                days,
                first,
                last,
            } => Self::DaysNotImported {
                source: source_name(*source),
                days: *days,
                first_day: first.to_string(),
                last_day: last.to_string(),
            },
            AiCloseProblem::AccountCheck {
                account_id,
                login,
                outcome,
            } => Self::AccountCheck {
                account_id: account_id.as_i64(),
                login: login.as_str().to_string(),
                outcome: outcome.into(),
            },
            AiCloseProblem::OrgCheck { outcome } => Self::OrgCheck {
                outcome: outcome.into(),
            },
            AiCloseProblem::UnlinkedCredits {
                account_id,
                login,
                credits,
            } => Self::UnlinkedCredits {
                account_id: account_id.as_i64(),
                login: login.as_str().to_string(),
                credits: credits_decimal(*credits),
            },
            AiCloseProblem::ResidualNotSettled { reasons: why } => Self::ResidualNotSettled {
                reasons: reasons(why),
            },
            AiCloseProblem::Residual { day, nanos } => Self::Residual {
                day: day.to_string(),
                credits: nanos_as_credits(*nanos),
            },
            AiCloseProblem::UnmappedRepository {
                repository,
                credits,
            } => Self::UnmappedRepository {
                repository: repository.clone(),
                credits: credits_decimal(*credits),
            },
            AiCloseProblem::UnrecognizedRepositoryRow => Self::UnrecognizedRepositoryRow {},
            AiCloseProblem::MeteredPricing {
                account_id,
                login,
                reasons: why,
            } => Self::MeteredPricing {
                account_id: account_id.as_i64(),
                login: login.as_str().to_string(),
                reasons: reasons(why),
            },
            AiCloseProblem::OldBasis {
                developer,
                machine_id,
                label,
                provider,
                buckets,
                first_day,
                last_day,
                estimate,
            } => Self::OldBasis {
                user_id: developer.user_id.as_i32(),
                full_name: developer.full_name.clone(),
                machine_id: machine_id.to_string(),
                label: label.clone(),
                provider: (*provider).into(),
                buckets: *buckets,
                first_day: first_day.to_string(),
                last_day: last_day.to_string(),
                estimate_usd: estimate.to_decimal(2),
            },
            AiCloseProblem::RateMissing { currency } => Self::RateMissing {
                currency: currency.as_str().to_string(),
            },
            AiCloseProblem::RatePending { currency } => Self::RatePending {
                currency: currency.as_str().to_string(),
            },
            AiCloseProblem::RateProvisional {
                currency,
                observed_through,
            } => Self::RateProvisional {
                currency: currency.as_str().to_string(),
                observed_through: observed_through.to_string(),
            },
            AiCloseProblem::AdjustedRateMissing { month, currency } => Self::AdjustedRateMissing {
                month: month.to_string(),
                currency: currency.as_str().to_string(),
            },
            AiCloseProblem::AdjustedRatePending { month, currency } => Self::AdjustedRatePending {
                month: month.to_string(),
                currency: currency.as_str().to_string(),
            },
            AiCloseProblem::AdjustedRateProvisional { month, currency } => {
                Self::AdjustedRateProvisional {
                    month: month.to_string(),
                    currency: currency.as_str().to_string(),
                }
            }
            AiCloseProblem::MachineIncomplete {
                developer,
                machine_id,
                label,
                gaps,
                providers,
            } => Self::MachineIncomplete {
                user_id: developer.user_id.as_i32(),
                full_name: developer.full_name.clone(),
                machine_id: machine_id.to_string(),
                label: label.clone(),
                gaps: gap_responses(gaps),
                providers: providers
                    .iter()
                    .map(|(provider, gaps)| ProviderGapsResponse {
                        provider: (*provider).into(),
                        gaps: gap_responses(gaps),
                    })
                    .collect(),
            },
            AiCloseProblem::UtcHoursNotUploaded {
                developer,
                machine_id,
                label,
                provider,
                from,
                until,
            } => Self::UtcHoursNotUploaded {
                user_id: developer.user_id.as_i32(),
                full_name: developer.full_name.clone(),
                machine_id: machine_id.to_string(),
                label: label.clone(),
                provider: provider.map(Into::into),
                from: *from,
                until: *until,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum CheckStatusResponse {
    Passed,
    Failed,
    /// Failed, and an admin waived it with a reason.
    Waived,
    NotApplicable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WaiverResponse {
    reason: String,
}

/// One readiness check.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResponse {
    /// `import`, `dispositions`, `unknownAccount`, `repositories`,
    /// `pricing`, `exchangeRates` or `uploads`.
    id: &'static str,
    status: CheckStatusResponse,
    problems: Vec<ProblemResponse>,
    /// Non-null exactly when `status` is `waived`.
    waiver: Option<WaiverResponse>,
}

impl From<&AiCloseCheck> for CheckResponse {
    fn from(check: &AiCloseCheck) -> Self {
        Self {
            id: check.id.as_str(),
            status: match (check.status, &check.waiver) {
                (AiCloseCheckStatus::Passed, _) => CheckStatusResponse::Passed,
                (AiCloseCheckStatus::Failed, Some(_)) => CheckStatusResponse::Waived,
                (AiCloseCheckStatus::Failed, None) => CheckStatusResponse::Failed,
                (AiCloseCheckStatus::NotApplicable, _) => CheckStatusResponse::NotApplicable,
            },
            problems: check.problems.iter().map(Into::into).collect(),
            waiver: check.waiver.as_ref().map(|reason| WaiverResponse {
                reason: reason.as_str().to_string(),
            }),
        }
    }
}

/// A month's readiness to close, as evaluated by the server.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseReadinessResponse {
    #[serde(with = "time::serde::rfc3339")]
    evaluated_at: OffsetDateTime,
    /// No check is failed without a waiver.
    ready: bool,
    /// In evaluation order.
    checks: Vec<CheckResponse>,
}

impl From<&AiCloseReadiness> for CloseReadinessResponse {
    fn from(readiness: &AiCloseReadiness) -> Self {
        Self {
            evaluated_at: readiness.evaluated_at,
            ready: readiness.ready(),
            checks: readiness.checks.iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum CloseStateResponse {
    Open,
    Closed,
}

/// A closed month's close.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusCloseResponse {
    close_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    closed_at: OffsetDateTime,
    closed_by: Option<String>,
    /// The readiness as it was when the month closed, with its waivers.
    readiness: Value,
    /// Where later changes to the month post, `YYYY-MM`.
    adjustments_post_on: String,
    /// Months whose closes hold adjustments to this one: it can be re-opened
    /// once they are.
    held_by: Vec<String>,
}

/// A past close of the month.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseHistoryResponse {
    close_id: i64,
    #[serde(with = "time::serde::rfc3339")]
    closed_at: OffsetDateTime,
    closed_by: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    reopened_at: Option<OffsetDateTime>,
    reopened_by: Option<String>,
}

/// Where a month stands with close.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CloseStatusResponse {
    /// `YYYY-MM`.
    month: String,
    state: CloseStateResponse,
    time_zone: String,
    /// An open month that is not over yet: when it is, in UTC and the
    /// billing time zone. Null once it is over, and for a closed month.
    #[serde(with = "time::serde::rfc3339::option")]
    over_at: Option<OffsetDateTime>,
    /// An open month that is over: its readiness now. Null otherwise.
    readiness: Option<CloseReadinessResponse>,
    /// An open month: the closed months whose changes post on it.
    adjusts: Vec<AdjustedMonthResponse>,
    /// A closed month's close; null while it is open.
    close: Option<StatusCloseResponse>,
    /// Every close of the month, re-opened ones included, newest first.
    history: Vec<CloseHistoryResponse>,
}

impl CloseStatusResponse {
    fn new(status: AiMonthCloseStatus, time_zone: &str) -> Self {
        let history = status
            .history
            .iter()
            .map(|close| CloseHistoryResponse {
                close_id: close.id.as_i64(),
                closed_at: close.closed_at,
                closed_by: close.closed_by.clone(),
                reopened_at: close.reopened_at,
                reopened_by: close.reopened_by.clone(),
            })
            .collect();
        let month = status.month.to_string();
        let time_zone = time_zone.to_string();
        match status.state {
            AiMonthCloseState::Open {
                over_at,
                readiness,
                adjusts,
            } => Self {
                month,
                state: CloseStateResponse::Open,
                time_zone,
                over_at,
                readiness: readiness.as_ref().map(Into::into),
                adjusts: adjusts.iter().map(Into::into).collect(),
                close: None,
                history,
            },
            AiMonthCloseState::Closed {
                close,
                readiness_json,
                adjustments_post_on,
                held_by,
            } => Self {
                month,
                state: CloseStateResponse::Closed,
                time_zone,
                over_at: None,
                readiness: None,
                adjusts: Vec::new(),
                close: Some(StatusCloseResponse {
                    close_id: close.id.as_i64(),
                    closed_at: close.closed_at,
                    closed_by: close.closed_by,
                    readiness: readiness_json,
                    adjustments_post_on: adjustments_post_on.to_string(),
                    held_by: held_by.iter().map(ToString::to_string).collect(),
                }),
                history,
            },
        }
    }
}

/// A close or re-open that could not happen, or a bad request for one.
pub(super) enum CloseError {
    Close(AiMonthCloseError),
    Request(ApiError),
}

impl From<AiMonthCloseError> for CloseError {
    fn from(error: AiMonthCloseError) -> Self {
        Self::Close(error)
    }
}

impl From<ApiError> for CloseError {
    fn from(error: ApiError) -> Self {
        Self::Request(error)
    }
}

impl IntoResponse for CloseError {
    fn into_response(self) -> Response {
        let error = match self {
            Self::Request(error) => return error.into_response(),
            Self::Close(error) => error,
        };
        let message = error.to_string();
        match error {
            AiMonthCloseError::Billing(error) => ApiError::from(error).into_response(),
            AiMonthCloseError::NotReady(readiness) => (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": message,
                    "readiness": CloseReadinessResponse::from(readiness.as_ref()),
                })),
            )
                .into_response(),
            AiMonthCloseError::HeldByAdjustments { held_by, .. } => (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": message,
                    "heldBy": held_by.iter().map(ToString::to_string).collect::<Vec<_>>(),
                })),
            )
                .into_response(),
            AiMonthCloseError::NotClosed { .. } => ApiError::not_found(message).into_response(),
            AiMonthCloseError::MonthInProgress { .. }
            | AiMonthCloseError::AlreadyClosed { .. }
            | AiMonthCloseError::Busy
            | AiMonthCloseError::ImportRunning => ApiError::conflict(message).into_response(),
        }
    }
}

/// Where a month stands with close, with its readiness once it is over.
pub(super) async fn close_status(
    State(service): State<Arc<dyn AiBillingService>>,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Json<CloseStatusResponse>, CloseError> {
    let month = parse_month(&month)?;
    let status = service.close_status(month).await?;
    Ok(Json(CloseStatusResponse::new(
        status,
        service.time_zone().as_str(),
    )))
}

/// An admin's waiver of a failed check.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WaiverRequest {
    check: String,
    reason: String,
}

/// Closes a month, waiving the failed checks named. A waiver of a check
/// that passed, or that holds a running import, is dropped.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CloseRequest {
    #[serde(default)]
    waivers: Vec<WaiverRequest>,
}

fn parse_waivers(requests: Vec<WaiverRequest>) -> Result<Vec<AiCloseWaiver>, ApiError> {
    let mut seen: HashSet<AiCloseCheckId> = HashSet::new();
    requests
        .into_iter()
        .map(|request| {
            let check = AiCloseCheckId::parse(&request.check).ok_or_else(|| {
                ApiError::bad_request(format!("unknown readiness check {:?}", request.check))
            })?;
            if !seen.insert(check) {
                return Err(ApiError::bad_request(format!(
                    "check {:?} is waived twice",
                    request.check
                )));
            }
            let reason = AiWaiverReason::parse(&request.reason).map_err(ApiError::bad_request)?;
            Ok(AiCloseWaiver { check, reason })
        })
        .collect()
}

/// Closes a month's whole AI bill: `201` with its close status.
pub(super) async fn close_month(
    State(service): State<Arc<dyn AiBillingService>>,
    user: AuthUser,
    WithRejection(Path(month), _): PathParam<String>,
    WithRejection(Json(body), _): JsonBody<CloseRequest>,
) -> Result<(StatusCode, Json<CloseStatusResponse>), CloseError> {
    let month = parse_month(&month)?;
    let waivers = parse_waivers(body.waivers)?;
    let projector = HttpBillProjector {
        time_zone: service.time_zone().as_str().to_string(),
        cutover: service.metered_cutover(),
    };
    let status = service
        .close_month(month, &user.id, waivers, &projector)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CloseStatusResponse::new(
            status,
            service.time_zone().as_str(),
        )),
    ))
}

/// Re-opens a closed month, so its bill is computed live again.
pub(super) async fn reopen_month(
    State(service): State<Arc<dyn AiBillingService>>,
    user: AuthUser,
    WithRejection(Path(month), _): PathParam<String>,
) -> Result<Json<CloseStatusResponse>, CloseError> {
    let month = parse_month(&month)?;
    let status = service.reopen_month(month, &user.id).await?;
    Ok(Json(CloseStatusResponse::new(
        status,
        service.time_zone().as_str(),
    )))
}
