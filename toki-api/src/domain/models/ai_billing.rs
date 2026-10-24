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
//! - **API days.** A local day that none of the developer's subscriptions that
//!   pay for the provider covers (`resolve_billing_mode` is `Api`) bills that day's usage at
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
//! - **Pi.** Pi has no subscriptions of its own. On a local day for which it
//!   reported its ChatGPT sign-in, its usage is paid for by the developer's
//!   subscription to Codex that covers the day, if any, and is API usage on any
//!   other day (`subscription_provider`). Such a subscription's fee is split
//!   over the Codex and the Pi usage it pays for alike: each project's usage of
//!   each of the two gets a line and a share by the same weights, and the
//!   subscription is overhead only without usage of either.
//! - **Explicit payers.** Usage whose bucket names a `billing_provider` is paid
//!   for by the developer's subscription to that provider covering the day, if
//!   any, and is API usage otherwise, alike for every source tool. OpenCode has
//!   no subscriptions of its own, so its usage without a payer is API usage.
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
//! - **Personal usage.** Usage of a project key its developer marked personal
//!   is left out before any of the above: it bills nothing on API days and
//!   takes no share of a subscription's fee, which is split over the remaining
//!   usage alone. A subscription whose covered usage is all personal bills as
//!   one without usage. It is reported apart, as an estimate only
//!   (`AiPersonalUsage`).
//!
//! Usage that no project mapping resolves, including `unattributed` usage,
//! belongs to the Unassigned project (`None`). It bills as one project, so
//! the bill is the same whatever made its usage Unassigned. The bill only
//! shows it apart by kind (`AiUnassignedKind`): `AiBillingLine::parts` splits
//! an Unassigned line's amounts, in its own currency and in the billing
//! currency, between the kinds by the largest-remainder method, so the parts
//! sum to the line exactly and no project's amount changes.

use std::collections::BTreeMap;
use std::fmt;

use crate::domain::AiBillingError;
use time::{Date, Month};

use super::{
    resolve_billing_mode, AiBillingDeveloper, AiBillingMode, AiConvertedAmount, AiConvertedBill,
    AiCurrency, AiMachineId, AiMappedProject, AiProvider, AiSubscription, AiSubscriptionId,
    AiTokenCounts, AiUsageDateRange, UserId, MAX_SAFE_COUNT,
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

    /// The month `day` is in, from year 1.
    pub fn containing(day: Date) -> Option<Self> {
        // Postgres dates have no year 0.
        (day.year() >= 1)
            .then(|| Self::new(day.year(), day.month()))
            .flatten()
    }

    /// The month before this one, from year 1.
    pub fn previous(&self) -> Option<Self> {
        self.first_day().previous_day().and_then(Self::containing)
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

/// Why usage is Unassigned, which decides what could assign it. In the order
/// the bill shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiUnassignedKind {
    /// Usage of project keys that no mapping to the configured company
    /// resolves: unmapped repositories, and keys mapped to a project of another
    /// company. Mapping its keys assigns it.
    Unmapped,
    /// `unattributed` usage: outside any repository, with no project metadata.
    /// No mapping can assign it.
    Unattributed,
}

/// Where a day's usage bills: the time-tracking project its key maps to, or
/// Unassigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiBillingProject {
    Mapped(AiMappedProject),
    Unassigned(AiUnassignedKind),
}

/// A developer's usage of one provider on one local day, attributed to the
/// time-tracking project its key maps to, or to one kind of Unassigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDailyUsage {
    /// Explicit subscription payer; the usage provider remains its source tool.
    pub billing_provider: Option<AiProvider>,
    pub user_id: UserId,
    pub provider: AiProvider,
    pub day: Date,
    /// Whether the provider reported its ChatGPT sign-in for the developer on
    /// this day (`PI_CHATGPT_SIGN_IN_HINT`). Only Pi does.
    pub chatgpt_sign_in: bool,
    pub project: AiBillingProject,
    pub usage: AiBillingUsage,
}

/// The usage of one kind of Unassigned within a billing line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiUnassignedUsage {
    pub kind: AiUnassignedKind,
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
    /// The provider whose usage the line charges for. A subscription line's
    /// can be Pi, on a subscription to Codex; overhead names the
    /// subscription's provider.
    pub provider: AiProvider,
    /// The time-tracking project, or `None` for Unassigned usage and for
    /// unallocated overhead.
    pub project: Option<AiMappedProject>,
    pub charge: AiBillingCharge,
    /// The usage the line charges for; zero for overhead.
    pub usage: AiBillingUsage,
    /// An Unassigned line's usage by kind, in kind order, of the kinds it has;
    /// they sum to `usage`. Empty for a project's line and for overhead.
    pub unassigned: Vec<AiUnassignedUsage>,
}

/// A billing line, or one kind of an Unassigned line's usage, as the bill
/// shows it (`AiBillingLine::parts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBillingLinePart<'a> {
    pub line: &'a AiBillingLine,
    /// The kind of Unassigned usage; `None` for a project's line and for
    /// overhead.
    pub unassigned: Option<AiUnassignedKind>,
    pub usage: AiBillingUsage,
    /// What the part bills, as `AiBillingLine::billable` for the line.
    pub billable: Option<AiFeeAmount>,
    /// What the part bills in the billing currency.
    pub converted: AiConvertedAmount,
}

impl AiBillingLine {
    /// The line as the bill shows it, given what it bills in the billing
    /// currency: the line itself, or for Unassigned usage of both kinds, one
    /// part per kind.
    ///
    /// The kinds split the line's amount and its converted amount apart, each
    /// by the largest-remainder method with weights as a subscription's fee
    /// is split (`allocation_weights` over the kinds' usage), ties going to
    /// Unmapped. The parts therefore sum to the line exactly in both
    /// currencies, but a part's cents need not be its own estimate rounded,
    /// and its converted amount need not be its own amount converted: two
    /// kinds of 0.004 USD each bill 0.01 and 0.00 USD, and at 10 SEK per USD
    /// 0.05 SEK each. An API part whose usage is all unpriced has an unknown
    /// amount, as an API line does; its weight, and so its share, is zero.
    pub fn parts(
        &self,
        converted: &AiConvertedAmount,
    ) -> Result<Vec<AiBillingLinePart<'_>>, AiBillingError> {
        if self.unassigned.len() < 2 {
            return Ok(vec![AiBillingLinePart {
                line: self,
                unassigned: self.unassigned.first().map(|part| part.kind),
                usage: self.usage,
                billable: self.billable(),
                converted: converted.clone(),
            }]);
        }

        let usages: Vec<AiBillingUsage> = self.unassigned.iter().map(|part| part.usage).collect();
        let (_, weights) = allocation_weights(&usages);
        let split = |total: i64| -> Result<Vec<i64>, AiBillingError> {
            match allocate_largest_remainder(total, &weights)? {
                Some(parts) => Ok(parts),
                None if total == 0 => Ok(vec![0; weights.len()]),
                None => Err(AiBillingError::Invalid(
                    "an Unassigned line bills an amount without usage to split it by".to_string(),
                )),
            }
        };
        let currency = match &self.charge {
            AiBillingCharge::Api => AiCurrency::usd(),
            AiBillingCharge::Subscription { fee, .. }
            | AiBillingCharge::UnallocatedOverhead { fee, .. } => fee.currency.clone(),
        };
        let shares = split(self.billable().map_or(0, |billed| billed.hundredths))?;
        let converted_shares = match converted {
            AiConvertedAmount::Converted { amount, .. } => Some(split(amount.hundredths)?),
            _ => None,
        };
        let zero = AiConvertedAmount::Converted {
            amount: AiFeeAmount::zero(AiCurrency::billing()),
            rate: None,
        };

        Ok(self
            .unassigned
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                let unknown = matches!(self.charge, AiBillingCharge::Api)
                    && kind.usage.priced_cost == AiUsdNanos::ZERO
                    && kind.usage.unpriced_records > 0;
                let billable = (!unknown).then(|| AiFeeAmount {
                    hundredths: shares[index],
                    currency: currency.clone(),
                });
                let converted = match (&billable, converted, &converted_shares) {
                    (None, _, _) => AiConvertedAmount::UnknownAmount,
                    (Some(_), AiConvertedAmount::Converted { amount, rate }, Some(shares)) => {
                        AiConvertedAmount::Converted {
                            amount: AiFeeAmount {
                                hundredths: shares[index],
                                currency: amount.currency.clone(),
                            },
                            rate: rate.clone(),
                        }
                    }
                    // Without a weight, the part takes no share of any amount.
                    (Some(_), _, _) if weights[index] == 0 => zero.clone(),
                    (Some(_), AiConvertedAmount::MissingRate { currency }, _) => {
                        AiConvertedAmount::MissingRate {
                            currency: currency.clone(),
                        }
                    }
                    // The line's amount is unknown, but not this part's.
                    (Some(_), _, _) => zero.clone(),
                };
                AiBillingLinePart {
                    line: self,
                    unassigned: Some(kind.kind),
                    usage: kind.usage,
                    billable,
                    converted,
                }
            })
            .collect())
    }

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
    /// The usage the subscription pays for on the covered days, over all
    /// projects and, for a subscription that also pays for Pi usage, providers.
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
    /// Ordered by user id and provider, a subscription line counting under its
    /// subscription's provider; within them, subscription lines by subscription
    /// start and the provider used, then API lines, each by project name with
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
    /// Unassigned usage by kind; empty for a project.
    unassigned: BTreeMap<AiUnassignedKind, AiBillingUsage>,
}

impl ProjectUsage {
    fn unassigned(&self) -> Vec<AiUnassignedUsage> {
        self.unassigned
            .iter()
            .map(|(kind, usage)| AiUnassignedUsage {
                kind: *kind,
                usage: *usage,
            })
            .collect()
    }
}

/// Usage per project, keyed by project id; `None` is Unassigned, whatever its
/// kind. Projects with one id and different names (renamed between two keys'
/// mappings) merge under the first name seen.
#[derive(Debug, Default)]
struct Projects(BTreeMap<Option<String>, ProjectUsage>);

impl Projects {
    fn add(
        &mut self,
        project: &AiBillingProject,
        usage: &AiBillingUsage,
    ) -> Result<(), AiBillingError> {
        let (mapped, kind) = match project {
            AiBillingProject::Mapped(project) => (Some(project), None),
            AiBillingProject::Unassigned(kind) => (None, Some(*kind)),
        };
        let entry = self
            .0
            .entry(mapped.map(|project| project.id.to_string()))
            .or_insert_with(|| ProjectUsage {
                project: mapped.cloned(),
                usage: AiBillingUsage::default(),
                unassigned: BTreeMap::new(),
            });
        entry.usage.add(usage)?;
        if let Some(kind) = kind {
            entry.unassigned.entry(kind).or_default().add(usage)?;
        }
        Ok(())
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
fn allocation_weights(usages: &[AiBillingUsage]) -> (AiAllocationBasis, Vec<u128>) {
    let weights = |weight: fn(&AiBillingUsage) -> i64| -> Vec<u128> {
        usages
            .iter()
            .map(|usage| {
                u128::try_from(weight(usage))
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
/// day's billing mode comes from `resolve_billing_mode` over all of them and
/// the day's `chatgpt_sign_in`.
/// A month whose counts or amounts cannot be represented safely returns a
/// range error; its estimates are never silently saturated.
pub fn bill_month(
    month: AiBillingMonth,
    usage: &[AiDailyUsage],
    subscriptions: &[AiSubscription],
) -> Result<AiMonthBill, AiBillingError> {
    /// Usage per user and provider, in the order of the bill's lines.
    type Usage = BTreeMap<(i32, &'static str), (UserId, AiProvider, Projects)>;
    fn projects(usage: &mut Usage, user_id: UserId, provider: AiProvider) -> &mut Projects {
        &mut usage
            .entry((user_id.as_i32(), provider.as_str()))
            .or_insert_with(|| (user_id, provider, Projects::default()))
            .2
    }

    // Usage on API days, and the usage each subscription pays for, which for a
    // subscription to Codex can be Pi usage too.
    let mut api = Usage::new();
    let mut subscribed: BTreeMap<i32, Usage> = BTreeMap::new();
    for row in usage.iter().filter(|row| month.contains(row.day)) {
        let billed = match resolve_billing_mode(
            subscriptions,
            row.user_id,
            row.billing_provider.unwrap_or(row.provider),
            row.day,
            row.chatgpt_sign_in,
        ) {
            AiBillingMode::Api => &mut api,
            AiBillingMode::Subscription(subscription) => {
                subscribed.entry(subscription.id.as_i32()).or_default()
            }
        };
        projects(billed, row.user_id, row.provider).add(&row.project, &row.usage)?;
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
        projects(&mut api, subscription.user_id, subscription.terms.provider);
    }

    let days_in_month = month.days();
    let mut lines = Vec::new();
    let mut subscription_months = Vec::new();
    for (user_id, provider, api_projects) in api.into_values() {
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
            // One line per provider used and project, in that order.
            let mut covered_usage = AiBillingUsage::default();
            let mut used_providers = Vec::new();
            let mut used_projects = Vec::new();
            for (_, used_provider, projects) in subscribed
                .remove(&subscription.id.as_i32())
                .unwrap_or_default()
                .into_values()
            {
                covered_usage.add(&projects.total()?)?;
                for project in projects.into_sorted() {
                    used_providers.push(used_provider);
                    used_projects.push(project);
                }
            }

            let used_usage: Vec<AiBillingUsage> =
                used_projects.iter().map(|project| project.usage).collect();
            let (basis, weights) = allocation_weights(&used_usage);
            let shares = allocate_largest_remainder(prorated_fee.hundredths, &weights)?;
            let (basis, weights) = match shares {
                Some(shares) => {
                    for ((used_provider, project), share) in
                        used_providers.into_iter().zip(used_projects).zip(shares)
                    {
                        lines.push(AiBillingLine {
                            user_id,
                            provider: used_provider,
                            unassigned: project.unassigned(),
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
                        unassigned: Vec::new(),
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

        for project in api_projects.into_sorted() {
            lines.push(AiBillingLine {
                user_id,
                provider,
                unassigned: project.unassigned(),
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
    /// Explicit subscription payer; the usage provider remains its source tool.
    pub billing_provider: Option<AiProvider>,
    pub day: Date,
    pub provider: AiProvider,
    /// As in `AiDailyUsage`.
    pub chatgpt_sign_in: bool,
    pub model: String,
    pub machine_id: AiMachineId,
    /// The project key, or `None` for the summed usage of every key without a
    /// mapping. Such a key may name a private repository, so admins learn its
    /// name only once a developer maps it. `unattributed` names nothing and is
    /// kept.
    pub project_key: Option<String>,
    /// The project the key maps to now, or `None` for Unassigned.
    pub project: Option<AiMappedProject>,
    pub usage: AiBillingUsage,
}

/// A developer's usage in a month of the project keys they marked personal.
/// It is left out of the bill; this is what it would have cost at API prices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiPersonalUsage {
    pub user_id: UserId,
    pub usage: AiBillingUsage,
}

/// Everyone's bill for a month, with the people it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthOverview {
    pub bill: AiMonthBill,
    /// The users the bill's lines, subscriptions and personal usage belong
    /// to, by name.
    pub developers: Vec<AiBillingDeveloper>,
    /// The bill in the billing currency, with the month's exchange rates.
    pub converted: AiConvertedBill,
    /// The usage the bill leaves out, by user.
    pub personal: Vec<AiPersonalUsage>,
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
    /// The developer's usage of the keys they marked personal, which the bill
    /// and `usage` leave out; `None` without any.
    pub personal: Option<AiBillingUsage>,
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
            let subscription_id = match resolve_billing_mode(
                subscriptions,
                user_id,
                usage.billing_provider.unwrap_or(usage.provider),
                usage.day,
                usage.chatgpt_sign_in,
            ) {
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
