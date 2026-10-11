//! Monthly billing of AI usage to time-tracking projects.
//!
//! Everything here is pure: the billing service loads usage per local day and
//! the declared subscriptions, and these functions decide what each developer's
//! usage of each provider costs each project in one calendar month of the
//! `ai_usage.time_zone` setting.
//!
//! # Billing rules
//!
//! Per developer, provider and month:
//!
//! - **API days.** A local day that none of the developer's subscriptions to the
//!   provider covers (`resolve_billing_mode` is `Api`) bills that day's usage at
//!   its estimated API-equivalent cost, in USD, to the project the usage is
//!   attributed to. Each billing line's amount is rounded half up to whole
//!   cents, and totals add up those rounded amounts, so an invoice built from
//!   the lines matches the totals; the unrounded estimate is kept apart.
//! - **Subscription days.** A subscription bills its monthly fee, in its own
//!   currency, pro-rated by the days of the month it covers:
//!   `fee × covered days / days in the month`, rounded half up to hundredths.
//!   A subscription that covers the whole month bills exactly its fee. When a
//!   plan changes mid-month, each subscription is pro-rated on its own days.
//! - **Split.** The pro-rated fee is split across projects in proportion to each
//!   project's API-equivalent cost on the days that subscription covers, in
//!   hundredths by the largest-remainder method, so the shares sum to the
//!   pro-rated fee exactly (`allocate_largest_remainder`). Each share is shown
//!   next to the API-equivalent cost it was weighted by.
//! - **Unpriced usage.** Records without a known price have no cost, which is
//!   unknown rather than zero. A split weighs the *priced* cost only, so a
//!   project whose covered usage is all unpriced gets no share; the line keeps
//!   its unpriced record count as a warning. When no covered usage has a
//!   positive priced cost, the split falls back to each project's share of
//!   tokens, and to its share of records if there are no tokens either
//!   (`AiAllocationBasis`). API days bill the priced subtotal, and their
//!   unpriced records remain an unknown amount on top.
//! - **No usage.** A subscription without usage on any day it covers in the
//!   month bills its pro-rated fee as unallocated overhead, not as zero. Days
//!   without usage of a subscription that has usage on other days are not
//!   overhead: the fee pays for the month, and the usage decides the split.
//! - **Currencies.** Fees stay in the subscription's currency and estimates stay
//!   in USD, and totals here are per currency. Converting the bill to the
//!   billing currency is a separate step at the month's exchange rates
//!   (`convert_bill` in `ai_exchange_rate`), which keeps these amounts.
//!
//! Usage that no project mapping resolves, including `unattributed` usage,
//! belongs to the Unassigned project (`None`).

use std::collections::BTreeMap;
use std::fmt;

use crate::domain::AiBillingError;
use time::{Date, Month};

use super::{
    resolve_billing_mode, AiBillingDeveloper, AiBillingMode, AiConvertedBill, AiCurrency,
    AiMachineId, AiMappedProject, AiProvider, AiSubscription, AiSubscriptionId, AiTokenCounts,
    AiUsageDateRange, UserId, MAX_SAFE_COUNT,
};

const NANOS_PER_USD: i64 = 1_000_000_000;

/// A calendar month in the configured time zone, such as `2026-09`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiBillingMonth {
    dates: AiUsageDateRange,
}

impl AiBillingMonth {
    /// The month, or `None` when its last day has no successor to end on.
    pub fn new(year: i32, month: Month) -> Option<Self> {
        let first_day = Date::from_calendar_date(year, month, 1).ok()?;
        let last_day = first_day.replace_day(month.length(year)).ok()?;

        AiUsageDateRange::from_inclusive(first_day, last_day).map(|dates| Self { dates })
    }

    /// Parses `YYYY-MM`, such as `2026-09`, from year 1.
    pub fn parse(raw: &str) -> Option<Self> {
        let (year, month) = raw.split_once('-')?;
        let digits = |text: &str, len: usize| {
            text.len() == len && text.bytes().all(|byte| byte.is_ascii_digit())
        };
        if !digits(year, 4) || !digits(month, 2) {
            return None;
        }

        // Postgres dates have no year 0.
        let year = year.parse::<i32>().ok().filter(|year| *year >= 1)?;
        let month = Month::try_from(month.parse::<u8>().ok()?).ok()?;
        Self::new(year, month)
    }

    /// The month's local dates, half-open: `end` is the first day of the next month.
    pub fn dates(&self) -> AiUsageDateRange {
        self.dates
    }

    pub fn first_day(&self) -> Date {
        self.dates.start()
    }

    pub fn last_day(&self) -> Date {
        self.dates.last_day()
    }

    pub fn days(&self) -> u8 {
        self.first_day().month().length(self.first_day().year())
    }

    pub fn contains(&self, day: Date) -> bool {
        self.dates.start() <= day && day < self.dates.end()
    }
}

impl fmt::Display for AiBillingMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let first_day = self.first_day();
        write!(
            f,
            "{:04}-{:02}",
            first_day.year(),
            u8::from(first_day.month())
        )
    }
}

/// An API-equivalent USD estimate in billionths of a dollar, so that sums are
/// exact and splits deterministic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AiUsdNanos(i64);

impl AiUsdNanos {
    pub const ZERO: Self = Self(0);

    /// An estimate of `nanos` billionths of a dollar, if it is not negative.
    pub fn new(nanos: i64) -> Option<Self> {
        (nanos >= 0).then_some(Self(nanos))
    }

    pub fn nanos(self) -> i64 {
        self.0
    }

    /// Dollars, for display.
    pub fn to_usd(self) -> f64 {
        self.0 as f64 / NANOS_PER_USD as f64
    }

    /// Dollars with `decimals` (at most 9) decimals, rounded half up, such as
    /// `12.3456`.
    pub fn to_decimal(self, decimals: u32) -> String {
        let decimals = decimals.min(9);
        let step = 10_i64.pow(9 - decimals);
        let rounded = self.0 / step + i64::from(self.0 % step >= (step + 1) / 2);
        if decimals == 0 {
            return rounded.to_string();
        }

        let scale = 10_i64.pow(decimals);
        format!(
            "{}.{:0width$}",
            rounded / scale,
            rounded % scale,
            width = decimals as usize
        )
    }

    /// Whole cents, rounded half up: what a billing line charges.
    pub fn to_cents(self) -> i64 {
        let step = NANOS_PER_USD / 100;
        self.0 / step + i64::from(self.0 % step >= step / 2)
    }

    fn checked_add(self, other: Self) -> Result<Self, AiBillingError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(AiBillingError::NumericRange)
    }
}

/// An amount in hundredths of a currency unit, such as an allocated fee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiFeeAmount {
    pub hundredths: i64,
    pub currency: AiCurrency,
}

impl AiFeeAmount {
    fn zero(currency: AiCurrency) -> Self {
        Self {
            hundredths: 0,
            currency,
        }
    }

    /// The amount with two decimals, without the currency, such as `550.00`.
    pub fn to_decimal(&self) -> String {
        format!("{}.{:02}", self.hundredths / 100, self.hundredths % 100)
    }
}

/// Usage summed over buckets. Unpriced records have no known cost, so while
/// `unpriced_records` is positive `priced_cost` is a known subtotal, not a total.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiBillingUsage {
    pub tokens: AiTokenCounts,
    pub records: i64,
    /// The API-equivalent cost of the priced records.
    pub priced_cost: AiUsdNanos,
    pub unpriced_records: i64,
}

impl AiBillingUsage {
    /// Adds usage exactly, or reports an unsupported total without changing self.
    pub fn add(&mut self, other: &Self) -> Result<(), AiBillingError> {
        let add = |a: i64, b: i64| a.checked_add(b).ok_or(AiBillingError::NumericRange);
        let total = Self {
            tokens: AiTokenCounts {
                input: add(self.tokens.input, other.tokens.input)?,
                cache_read: add(self.tokens.cache_read, other.tokens.cache_read)?,
                cache_write: add(self.tokens.cache_write, other.tokens.cache_write)?,
                output: add(self.tokens.output, other.tokens.output)?,
            },
            records: add(self.records, other.records)?,
            priced_cost: self.priced_cost.checked_add(other.priced_cost)?,
            unpriced_records: add(self.unpriced_records, other.unpriced_records)?,
        };
        total.ensure_reportable()?;
        *self = total;
        Ok(())
    }

    /// Counts in reports must remain exact in the JavaScript consumers.
    pub fn ensure_reportable(&self) -> Result<(), AiBillingError> {
        let counts = [
            self.tokens.input,
            self.tokens.cache_read,
            self.tokens.cache_write,
            self.tokens.output,
        ];
        let total = counts
            .into_iter()
            .try_fold(0_i64, |total, count| total.checked_add(count))
            .ok_or(AiBillingError::NumericRange)?;
        if counts
            .into_iter()
            .chain([total, self.records, self.unpriced_records])
            .any(|count| !(0..=MAX_SAFE_COUNT).contains(&count))
        {
            return Err(AiBillingError::NumericRange);
        }
        Ok(())
    }

    /// Every token category together.
    pub fn total_tokens(&self) -> i64 {
        let AiTokenCounts {
            input,
            cache_read,
            cache_write,
            output,
        } = self.tokens;
        input + cache_read + cache_write + output
    }
}

/// A developer's usage of one provider on one local day, attributed to the
/// time-tracking project its key maps to, or to Unassigned (`None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDailyUsage {
    pub user_id: UserId,
    pub provider: AiProvider,
    pub day: Date,
    pub project: Option<AiMappedProject>,
    pub usage: AiBillingUsage,
}

/// What a billing line charges its project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiBillingCharge {
    /// Usage on days without a subscription, billed at its API-equivalent
    /// estimate in USD: the line's priced cost, plus any unpriced records at an
    /// unknown cost.
    Api,
    /// The project's share of a subscription's pro-rated fee.
    Subscription {
        subscription_id: AiSubscriptionId,
        fee: AiFeeAmount,
    },
    /// The pro-rated fee of a subscription without usage on any day it covers
    /// in the month. It has no project.
    UnallocatedOverhead {
        subscription_id: AiSubscriptionId,
        fee: AiFeeAmount,
    },
}

/// One charge of a developer's usage of a provider to one project in a month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBillingLine {
    pub user_id: UserId,
    pub provider: AiProvider,
    /// The time-tracking project, or `None` for Unassigned usage and for
    /// unallocated overhead.
    pub project: Option<AiMappedProject>,
    pub charge: AiBillingCharge,
    /// The usage the line charges for; zero for overhead.
    pub usage: AiBillingUsage,
}

impl AiBillingLine {
    /// What the line bills: a fee share, or an API line's estimate rounded to
    /// whole US cents. `None` for an API line whose usage is all unpriced, so
    /// its cost is unknown, not zero.
    pub fn billable(&self) -> Option<AiFeeAmount> {
        match &self.charge {
            AiBillingCharge::Api => {
                let unknown =
                    self.usage.priced_cost == AiUsdNanos::ZERO && self.usage.unpriced_records > 0;
                (!unknown).then(|| AiFeeAmount {
                    hundredths: self.usage.priced_cost.to_cents(),
                    currency: AiCurrency::usd(),
                })
            }
            AiBillingCharge::Subscription { fee, .. }
            | AiBillingCharge::UnallocatedOverhead { fee, .. } => Some(fee.clone()),
        }
    }
}

/// What a subscription's pro-rated fee was split by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiAllocationBasis {
    /// Each project's priced API-equivalent cost on the covered days.
    ApiCost,
    /// Each project's tokens, because no covered usage has a positive priced cost.
    Tokens,
    /// Each project's records, because the covered usage has neither a positive
    /// priced cost nor tokens.
    Records,
}

/// A subscription's charge for one month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSubscriptionMonth {
    pub subscription: AiSubscription,
    /// The days of the month the subscription covers.
    pub covered: AiUsageDateRange,
    pub days_in_month: u8,
    /// The monthly fee pro-rated by the covered days, in the fee's currency.
    pub prorated_fee: AiFeeAmount,
    /// How the fee was split, or `None` when nothing used the subscription and
    /// the whole fee is unallocated overhead.
    pub basis: Option<AiAllocationBasis>,
    /// The weights the fee was split by, one per subscription line of the bill,
    /// in the order of those lines; empty for overhead. Any other split of this
    /// fee, such as in another currency, uses these same weights.
    pub weights: Vec<u128>,
    /// Usage on the covered days, over all projects.
    pub usage: AiBillingUsage,
}

impl AiSubscriptionMonth {
    pub fn covered_days(&self) -> i64 {
        (self.covered.end() - self.covered.start()).whole_days()
    }
}

/// Everything a month bills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthBill {
    pub month: AiBillingMonth,
    /// Ordered by user id and provider; within them, subscription lines by
    /// subscription start, then API lines, each by project name with
    /// Unassigned last.
    pub lines: Vec<AiBillingLine>,
    /// Subscriptions that cover days of the month, by user id, provider and start.
    pub subscriptions: Vec<AiSubscriptionMonth>,
}

/// Month totals. Amounts in different currencies are never added together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthTotals {
    /// Fees billed per currency, allocated shares and overhead together, in
    /// currency code order.
    pub fees: Vec<AiCurrencyTotal>,
    /// What API lines bill, in USD: the sum of their amounts rounded to cents.
    pub api_billed: AiFeeAmount,
    /// Records on API days whose cost is unknown and not in `api_billed`.
    pub api_unpriced_records: i64,
    /// All usage in the month, whichever way it was billed, with its
    /// unrounded API-equivalent estimate.
    pub usage: AiBillingUsage,
}

/// Fees in one currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCurrencyTotal {
    /// Every fee billed: allocated shares and overhead.
    pub billed: AiFeeAmount,
    /// The part of `billed` that is unallocated overhead.
    pub overhead: AiFeeAmount,
}

impl AiMonthBill {
    /// Exact totals, or a range error when the month cannot be reported safely.
    pub fn totals(&self) -> Result<AiMonthTotals, AiBillingError> {
        let mut fees: BTreeMap<String, AiCurrencyTotal> = BTreeMap::new();
        let mut totals = AiMonthTotals {
            fees: Vec::new(),
            api_billed: AiFeeAmount::zero(AiCurrency::usd()),
            api_unpriced_records: 0,
            usage: AiBillingUsage::default(),
        };
        for line in &self.lines {
            totals.usage.add(&line.usage)?;
            match &line.charge {
                AiBillingCharge::Api => {
                    if let Some(billed) = line.billable() {
                        totals.api_billed.hundredths = totals
                            .api_billed
                            .hundredths
                            .checked_add(billed.hundredths)
                            .ok_or(AiBillingError::NumericRange)?;
                    }
                    totals.api_unpriced_records = totals
                        .api_unpriced_records
                        .checked_add(line.usage.unpriced_records)
                        .ok_or(AiBillingError::NumericRange)?;
                }
                AiBillingCharge::Subscription { fee, .. }
                | AiBillingCharge::UnallocatedOverhead { fee, .. } => {
                    let total = fees
                        .entry(fee.currency.as_str().to_string())
                        .or_insert_with(|| AiCurrencyTotal {
                            billed: AiFeeAmount::zero(fee.currency.clone()),
                            overhead: AiFeeAmount::zero(fee.currency.clone()),
                        });
                    total.billed.hundredths = total
                        .billed
                        .hundredths
                        .checked_add(fee.hundredths)
                        .ok_or(AiBillingError::NumericRange)?;
                    if matches!(line.charge, AiBillingCharge::UnallocatedOverhead { .. }) {
                        total.overhead.hundredths = total
                            .overhead
                            .hundredths
                            .checked_add(fee.hundredths)
                            .ok_or(AiBillingError::NumericRange)?;
                    }
                }
            }
        }
        totals.fees = fees.into_values().collect();

        Ok(totals)
    }
}

/// Pro-rates a monthly fee in hundredths: `fee × covered_days / days_in_month`,
/// rounded half up. A subscription covering the whole month bills its whole fee.
/// An amount outside the supported integer representation is a range error.
pub fn prorate_fee(
    monthly_hundredths: i64,
    covered_days: i64,
    days_in_month: i64,
) -> Result<i64, AiBillingError> {
    if days_in_month <= 0 {
        return Ok(0);
    }
    let numerator =
        i128::from(monthly_hundredths) * i128::from(covered_days) * 2 + i128::from(days_in_month);
    let prorated = numerator / (2 * i128::from(days_in_month));

    i64::try_from(prorated).map_err(|_| AiBillingError::NumericRange)
}

/// Splits `total` hundredths in proportion to `weights` by the largest-remainder
/// method: each part first gets the floor of its exact share, then the
/// hundredths left over go one each to the parts with the largest remainders,
/// ties going to the earlier part. The parts sum to `total` exactly. `None`
/// when the weights sum to zero or `total` is negative.
/// Arithmetic outside the supported representation is a range error, never
/// the absence of usage.
pub fn allocate_largest_remainder(
    total: i64,
    weights: &[u128],
) -> Result<Option<Vec<i64>>, AiBillingError> {
    let Ok(total_units) = u128::try_from(total) else {
        return Ok(None);
    };
    let weight_sum = weights
        .iter()
        .try_fold(0_u128, |sum, weight| sum.checked_add(*weight))
        .ok_or(AiBillingError::NumericRange)?;
    if weight_sum == 0 {
        return Ok(None);
    }

    let mut parts = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    for (index, weight) in weights.iter().enumerate() {
        let exact = total_units
            .checked_mul(*weight)
            .ok_or(AiBillingError::NumericRange)?;
        parts.push(exact / weight_sum);
        remainders.push((exact % weight_sum, index));
    }

    let floored: u128 = parts.iter().sum();
    // Fewer than one hundredth per part is left over.
    let left_over =
        usize::try_from(total_units - floored).map_err(|_| AiBillingError::NumericRange)?;
    remainders.sort_by(|(a, a_index), (b, b_index)| b.cmp(a).then(a_index.cmp(b_index)));
    for (_, index) in remainders.into_iter().take(left_over) {
        parts[index] += 1;
    }

    let parts = parts
        .into_iter()
        .map(|part| i64::try_from(part).map_err(|_| AiBillingError::NumericRange))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(parts))
}

/// Usage of one project within a group of days.
#[derive(Debug, Clone)]
struct ProjectUsage {
    project: Option<AiMappedProject>,
    usage: AiBillingUsage,
}

/// Usage per project, keyed by project id; `None` is Unassigned. Projects
/// with one id and different names (renamed between two keys' mappings) merge
/// under the first name seen.
#[derive(Debug, Default)]
struct Projects(BTreeMap<Option<String>, ProjectUsage>);

impl Projects {
    fn add(
        &mut self,
        project: &Option<AiMappedProject>,
        usage: &AiBillingUsage,
    ) -> Result<(), AiBillingError> {
        let key = project.as_ref().map(|project| project.id.to_string());
        self.0
            .entry(key)
            .or_insert_with(|| ProjectUsage {
                project: project.clone(),
                usage: AiBillingUsage::default(),
            })
            .usage
            .add(usage)
    }

    /// By project name, then id, with Unassigned last.
    fn into_sorted(self) -> Vec<ProjectUsage> {
        let mut projects: Vec<ProjectUsage> = self.0.into_values().collect();
        projects.sort_by(|a, b| match (&a.project, &b.project) {
            (Some(a), Some(b)) => a
                .name
                .cmp(&b.name)
                .then_with(|| a.id.as_str().cmp(b.id.as_str())),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        projects
    }

    fn total(&self) -> Result<AiBillingUsage, AiBillingError> {
        let mut total = AiBillingUsage::default();
        for project in self.0.values() {
            total.add(&project.usage)?;
        }
        Ok(total)
    }
}

/// The weights a subscription's fee is split by, and what they measure.
fn allocation_weights(projects: &[ProjectUsage]) -> (AiAllocationBasis, Vec<u128>) {
    let weights = |weight: fn(&AiBillingUsage) -> i64| -> Vec<u128> {
        projects
            .iter()
            .map(|project| {
                u128::try_from(weight(&project.usage))
                    .expect("billing usage is validated before allocating a fee")
            })
            .collect()
    };

    let cost = weights(|usage| usage.priced_cost.nanos());
    if cost.iter().any(|weight| *weight > 0) {
        return (AiAllocationBasis::ApiCost, cost);
    }
    let tokens = weights(AiBillingUsage::total_tokens);
    if tokens.iter().any(|weight| *weight > 0) {
        return (AiAllocationBasis::Tokens, tokens);
    }

    (AiAllocationBasis::Records, weights(|usage| usage.records))
}

/// Bills a month of usage.
///
/// `usage` holds any users' daily usage; days outside `month` are ignored.
/// `subscriptions` may hold any users' subscriptions to any provider; each
/// day's billing mode comes from `resolve_billing_mode` over all of them.
/// A month whose counts or amounts cannot be represented safely returns a
/// range error; its estimates are never silently saturated.
pub fn bill_month(
    month: AiBillingMonth,
    usage: &[AiDailyUsage],
    subscriptions: &[AiSubscription],
) -> Result<AiMonthBill, AiBillingError> {
    /// Usage of one user and provider, by how its days are billed.
    #[derive(Default)]
    struct Group {
        api: Projects,
        by_subscription: BTreeMap<i32, Projects>,
    }
    type Groups = BTreeMap<(i32, &'static str), (UserId, AiProvider, Group)>;
    fn group(groups: &mut Groups, user_id: UserId, provider: AiProvider) -> &mut Group {
        &mut groups
            .entry((user_id.as_i32(), provider.as_str()))
            .or_insert_with(|| (user_id, provider, Group::default()))
            .2
    }

    let mut groups = Groups::new();
    for row in usage.iter().filter(|row| month.contains(row.day)) {
        let group = group(&mut groups, row.user_id, row.provider);
        match resolve_billing_mode(subscriptions, row.user_id, row.provider, row.day) {
            AiBillingMode::Api => group.api.add(&row.project, &row.usage)?,
            AiBillingMode::Subscription(subscription) => group
                .by_subscription
                .entry(subscription.id.as_i32())
                .or_default()
                .add(&row.project, &row.usage)?,
        }
    }

    // Subscriptions that cover days of the month, earliest first.
    let mut covering: Vec<(&AiSubscription, AiUsageDateRange)> = subscriptions
        .iter()
        .filter_map(|subscription| {
            let covered = subscription.terms.period.to_date_range(month.dates())?;
            Some((subscription, covered))
        })
        .collect();
    covering.sort_by_key(|(subscription, _)| {
        (
            subscription.user_id.as_i32(),
            subscription.terms.provider.as_str(),
            subscription.terms.period.valid_from(),
            subscription.id.as_i32(),
        )
    });
    for (subscription, _) in &covering {
        group(
            &mut groups,
            subscription.user_id,
            subscription.terms.provider,
        );
    }

    let days_in_month = month.days();
    let mut lines = Vec::new();
    let mut subscription_months = Vec::new();
    for (user_id, provider, mut group) in groups.into_values() {
        for (subscription, covered) in covering.iter().filter(|(subscription, _)| {
            subscription.user_id == user_id && subscription.terms.provider == provider
        }) {
            let covered_days = (covered.end() - covered.start()).whole_days();
            let prorated_fee = AiFeeAmount {
                hundredths: prorate_fee(
                    subscription.terms.monthly_cost.hundredths(),
                    covered_days,
                    i64::from(days_in_month),
                )?,
                currency: subscription.terms.currency.clone(),
            };
            let projects = group
                .by_subscription
                .remove(&subscription.id.as_i32())
                .unwrap_or_default();
            let covered_usage = projects.total()?;
            let projects = projects.into_sorted();

            let (basis, weights) = allocation_weights(&projects);
            let shares = allocate_largest_remainder(prorated_fee.hundredths, &weights)?;
            let (basis, weights) = match shares {
                Some(shares) => {
                    for (project, share) in projects.into_iter().zip(shares) {
                        lines.push(AiBillingLine {
                            user_id,
                            provider,
                            project: project.project,
                            charge: AiBillingCharge::Subscription {
                                subscription_id: subscription.id,
                                fee: AiFeeAmount {
                                    hundredths: share,
                                    currency: prorated_fee.currency.clone(),
                                },
                            },
                            usage: project.usage,
                        });
                    }
                    (Some(basis), weights)
                }
                None => {
                    lines.push(AiBillingLine {
                        user_id,
                        provider,
                        project: None,
                        charge: AiBillingCharge::UnallocatedOverhead {
                            subscription_id: subscription.id,
                            fee: prorated_fee.clone(),
                        },
                        usage: AiBillingUsage::default(),
                    });
                    (None, Vec::new())
                }
            };

            subscription_months.push(AiSubscriptionMonth {
                subscription: (*subscription).clone(),
                covered: *covered,
                days_in_month,
                prorated_fee,
                basis,
                weights,
                usage: covered_usage,
            });
        }

        for project in group.api.into_sorted() {
            lines.push(AiBillingLine {
                user_id,
                provider,
                project: project.project,
                charge: AiBillingCharge::Api,
                usage: project.usage,
            });
        }
    }

    let bill = AiMonthBill {
        month,
        lines,
        subscriptions: subscription_months,
    };
    // Overview, CSV and drill-down must reject the same unsupported month.
    bill.totals()?;
    Ok(bill)
}

/// A developer's usage on one local day, as fine as admins may see it: by
/// provider, model, machine and project, never by hour or session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDeveloperDayUsage {
    pub day: Date,
    pub provider: AiProvider,
    pub model: String,
    pub machine_id: AiMachineId,
    pub project_key: String,
    /// The project the key maps to now, or `None` for Unassigned.
    pub project: Option<AiMappedProject>,
    pub usage: AiBillingUsage,
}

/// Everyone's bill for a month, with the people it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthOverview {
    pub bill: AiMonthBill,
    /// The users the bill's lines and subscriptions belong to, by name.
    pub developers: Vec<AiBillingDeveloper>,
    /// The bill in the billing currency, with the month's exchange rates.
    pub converted: AiConvertedBill,
}

/// A developer's day usage with the subscription that pays for it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBilledDayUsage {
    pub usage: AiDeveloperDayUsage,
    /// `None` when the day bills as API usage.
    pub subscription_id: Option<AiSubscriptionId>,
}

/// A machine's display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMachineLabel {
    pub machine_id: AiMachineId,
    pub label: String,
}

/// One developer's month: their bill and their usage per local day by
/// provider, model, machine and project. Never finer than a day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDeveloperMonth {
    pub developer: AiBillingDeveloper,
    pub bill: AiMonthBill,
    /// The bill in the billing currency, with the month's exchange rates.
    pub converted: AiConvertedBill,
    pub usage: Vec<AiBilledDayUsage>,
    /// The developer's machines, including ones without usage in the month.
    pub machines: Vec<AiMachineLabel>,
}

/// Marks each day's usage with the subscription that pays for it, as
/// `resolve_billing_mode` decides.
pub fn billed_day_usage(
    user_id: UserId,
    usage: Vec<AiDeveloperDayUsage>,
    subscriptions: &[AiSubscription],
) -> Vec<AiBilledDayUsage> {
    usage
        .into_iter()
        .map(|usage| {
            let subscription_id =
                match resolve_billing_mode(subscriptions, user_id, usage.provider, usage.day) {
                    AiBillingMode::Api => None,
                    AiBillingMode::Subscription(subscription) => Some(subscription.id),
                };
            AiBilledDayUsage {
                usage,
                subscription_id,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
