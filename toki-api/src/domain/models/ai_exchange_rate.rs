//! Converting a month's AI bill to the billing currency.
//!
//! Everything here is pure. The billing service loads the month's exchange
//! rates, fetching missing ones through an `ExchangeRateProvider`, and these
//! functions decide which rate to fetch and what each billing line bills in the
//! billing currency (`BILLING_CURRENCY`).
//!
//! # Rate rules
//!
//! - **One rate per month and currency**, stored with the month, so a bill can
//!   be reproduced: the provider's average of the month's daily rates.
//! - **Final and provisional.** Once a month is over, its monthly average is
//!   fetched once and is final. While a month is in progress, its rate is the
//!   average of the days published so far and is provisional. It is refetched
//!   once the provider may have published a newer day, and at most once an
//!   hour (`rate_to_fetch`). A month that is over but whose average is not
//!   published yet keeps a provisional average of its days, retried hourly.
//! - **Admin overrides.** An admin can override a month's rate. The override
//!   is kept apart from the fetched rate and takes precedence; fetches never
//!   touch it. Resetting it removes only the override, so the fetched rate
//!   applies again, refreshed only when it is due.
//! - **Missing rates.** A rate that is neither stored nor fetchable is missing,
//!   never guessed: the lines it would convert have no amount in the billing
//!   currency, and totals that include them are unknown.
//!
//! # Conversion rules
//!
//! - A line in another currency bills `amount × rate`, rounded half up to
//!   hundredths of the billing currency (öre). For an API line, the amount is
//!   what it bills in USD, whole cents, so `billable_amount × rate` reproduces
//!   the converted amount from the export.
//! - A subscription's **pro-rated fee** is converted once, and that amount is
//!   split across the subscription's lines with the weights that split the
//!   original fee (`AiSubscriptionMonth::weights`) and the same
//!   largest-remainder method. The converted shares sum exactly to the
//!   converted fee.
//! - Amounts already in the billing currency, and zero amounts, need no rate.
//! - Totals in the billing currency are sums of the converted lines.

use std::collections::BTreeMap;

use time::{Date, Duration, OffsetDateTime};

use crate::domain::AiBillingError;

use super::{
    allocate_largest_remainder, AiBillingCharge, AiBillingLine, AiBillingMonth, AiCurrency,
    AiFeeAmount, AiMonthBill,
};

/// The currency AI usage is billed in. Every billable amount is shown in it,
/// next to its original amount and currency.
pub const BILLING_CURRENCY: &str = "SEK";

/// A provisional rate is refetched at most this often.
pub const RATE_REFRESH_INTERVAL: Duration = Duration::HOUR;

const MILLIONTHS: i64 = 1_000_000;
/// The largest rate accepted: a million units of the billing currency per unit.
pub const MAX_RATE_UNITS: i64 = 1_000_000;
const MAX_RATE_MILLIONTHS: i64 = MAX_RATE_UNITS * MILLIONTHS;

/// Units of the billing currency per unit of another currency, exact to a
/// millionth, such as `9.50843` SEK per USD. Above 0 and at most
/// `MAX_RATE_UNITS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AiExchangeRateValue(i64);

impl AiExchangeRateValue {
    /// A rate of `millionths` millionths, if it is above 0 and at most
    /// `MAX_RATE_UNITS`.
    pub fn from_millionths(millionths: i64) -> Option<Self> {
        (millionths > 0 && millionths <= MAX_RATE_MILLIONTHS).then_some(Self(millionths))
    }

    /// Parses a plain decimal with at most six decimals, such as `9.50843` or
    /// `11`. No sign, exponent or thousands separators.
    pub fn parse(raw: &str) -> Option<Self> {
        let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
        let digits = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
        if whole.is_empty() || whole.len() > 7 || fraction.len() > 6 {
            return None;
        }
        if !digits(whole) || !digits(fraction) || (raw.contains('.') && fraction.is_empty()) {
            return None;
        }

        let whole: i64 = whole.parse().ok()?;
        let fraction: i64 = format!("{fraction:0<6}").parse().ok()?;
        Self::from_millionths(whole.checked_mul(MILLIONTHS)?.checked_add(fraction)?)
    }

    pub fn millionths(self) -> i64 {
        self.0
    }

    /// The rate with as many decimals as it needs and at least two, such as
    /// `9.50843` or `11.00`.
    pub fn to_decimal(self) -> String {
        let fraction = format!("{:06}", self.0 % MILLIONTHS);
        let trimmed = fraction.trim_end_matches('0');
        let fraction = if trimmed.len() < 2 {
            &fraction[..2]
        } else {
            trimmed
        };
        format!("{}.{fraction}", self.0 / MILLIONTHS)
    }

    /// Converts `hundredths` of the rate's currency to hundredths of the
    /// billing currency, rounded half up. `None` for a negative amount or on
    /// overflow.
    pub fn convert(self, hundredths: i64) -> Option<i64> {
        let hundredths = u128::try_from(hundredths).ok()?;
        let rate = u128::try_from(self.0).ok()?;
        let millionths = u128::try_from(MILLIONTHS).ok()?;
        let converted = (hundredths.checked_mul(rate)? + millionths / 2) / millionths;
        i64::try_from(converted).ok()
    }

    /// The mean of daily rates, rounded half up to a millionth. `None` without
    /// any.
    pub fn mean(values: &[Self]) -> Option<Self> {
        let count = i128::try_from(values.len())
            .ok()
            .filter(|count| *count > 0)?;
        let sum: i128 = values.iter().map(|value| i128::from(value.0)).sum();
        let mean = (2 * sum + count) / (2 * count);
        Self::from_millionths(i64::try_from(mean).ok()?)
    }
}

/// A rate as a provider returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiProvidedRate {
    pub rate: AiExchangeRateValue,
    /// The latest day whose daily rate the average includes.
    pub observed_through: Date,
}

/// A rate fetched from the exchange-rate provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiFetchedExchangeRate {
    pub rate: AiExchangeRateValue,
    /// The provider's name, such as `riksbank`.
    pub provider: String,
    /// An average of the days published so far, which later days change.
    pub provisional: bool,
    /// The latest day whose daily rate the average includes.
    pub observed_through: Date,
    pub fetched_at: OffsetDateTime,
    /// The local date of `fetched_at`.
    pub fetched_on: Date,
}

/// An admin's override of a month's rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExchangeRateOverride {
    pub rate: AiExchangeRateValue,
    /// The local date it was set on.
    pub set_on: Date,
    /// The admin who set it, if they still exist.
    pub by: Option<String>,
}

/// A month's rates for one currency: the fetched rate and an admin's
/// override, kept apart. At least one is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExchangeRate {
    pub month: AiBillingMonth,
    pub currency: AiCurrency,
    pub fetched: Option<AiFetchedExchangeRate>,
    pub overridden: Option<AiExchangeRateOverride>,
}

impl AiExchangeRate {
    /// The rate the month bills at: the override, else the fetched rate.
    pub fn rate(&self) -> Option<AiExchangeRateValue> {
        self.overridden
            .as_ref()
            .map(|overridden| overridden.rate)
            .or_else(|| self.fetched.as_ref().map(|fetched| fetched.rate))
    }

    pub fn is_override(&self) -> bool {
        self.overridden.is_some()
    }

    /// Whether the rate billed at is a provisional fetched rate.
    pub fn is_provisional(&self) -> bool {
        self.overridden.is_none() && self.fetched.as_ref().is_some_and(|f| f.provisional)
    }

    /// Where the rate billed at comes from: `admin`, or the provider's name.
    pub fn source(&self) -> &str {
        match (&self.overridden, &self.fetched) {
            (Some(_), _) => "admin",
            (None, Some(fetched)) => &fetched.provider,
            (None, None) => "",
        }
    }
}

/// What to ask the exchange-rate provider for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiRateFetch {
    /// The month is over: its final monthly average. If the provider has not
    /// published it yet, the average of the month's days is provisional.
    MonthAverage,
    /// The month is in progress: the provisional average of its days so far.
    MonthToDate { through: Date },
}

/// The store's clock: now, and today's date in the billing time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiRateClock {
    pub now: OffsetDateTime,
    pub today: Date,
}

/// Whether the fetched rate of a month needs fetching, and what to fetch.
///
/// `may_have_published(day)` says whether the provider may have published a
/// daily rate for a day after `day` by now.
///
/// - Nothing is fetched before the month begins, nor while an admin's
///   override applies, nor once a final rate is stored.
/// - Without a fetched rate, it is fetched.
/// - A provisional rate fetched while the month was in progress is refetched
///   as soon as the month is over.
/// - Otherwise a provisional rate is refetched at most once an hour
///   (`RATE_REFRESH_INTERVAL`): while the month is in progress, once a daily
///   rate newer than the ones it includes may be published; once the month is
///   over, until its final average is published.
pub fn rate_to_fetch(
    stored: Option<&AiExchangeRate>,
    month: AiBillingMonth,
    clock: AiRateClock,
    may_have_published: impl Fn(Date) -> bool,
) -> Option<AiRateFetch> {
    if month.first_day() > clock.today {
        return None;
    }
    let month_over = month.last_day() < clock.today;
    let fetch = if month_over {
        AiRateFetch::MonthAverage
    } else {
        AiRateFetch::MonthToDate {
            through: clock.today,
        }
    };
    let Some(stored) = stored else {
        return Some(fetch);
    };
    if stored.is_override() {
        return None;
    }
    let Some(fetched) = &stored.fetched else {
        return Some(fetch);
    };
    if !fetched.provisional {
        return None;
    }
    if month_over && fetched.fetched_on <= month.last_day() {
        return Some(fetch);
    }
    if clock.now - fetched.fetched_at < RATE_REFRESH_INTERVAL {
        return None;
    }
    (month_over || may_have_published(fetched.observed_through)).then_some(fetch)
}

/// The currencies other than the billing currency that a bill has non-zero
/// amounts in: the ones it needs rates for. A zero amount converts to zero.
pub fn currencies_to_convert(bill: &AiMonthBill) -> Vec<AiCurrency> {
    let mut currencies: BTreeMap<String, AiCurrency> = BTreeMap::new();
    let amounts = bill.lines.iter().filter_map(AiBillingLine::billable).chain(
        bill.subscriptions
            .iter()
            .map(|month| month.prorated_fee.clone()),
    );
    for amount in amounts {
        if amount.currency.as_str() != BILLING_CURRENCY && amount.hundredths != 0 {
            currencies.insert(amount.currency.as_str().to_string(), amount.currency);
        }
    }
    currencies.into_values().collect()
}

/// A line's amount in the billing currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiConvertedAmount {
    /// Converted at `rate`, or `None` when the line is in the billing currency
    /// or zero.
    Converted {
        amount: AiFeeAmount,
        rate: Option<AiExchangeRate>,
    },
    /// The line's own amount is unknown: all of its usage is unpriced.
    UnknownAmount,
    /// No rate for the line's currency: its amount in the billing currency is
    /// unknown, not zero.
    MissingRate { currency: AiCurrency },
}

impl AiConvertedAmount {
    pub fn amount(&self) -> Option<&AiFeeAmount> {
        match self {
            Self::Converted { amount, .. } => Some(amount),
            _ => None,
        }
    }

    pub fn rate(&self) -> Option<&AiExchangeRate> {
        match self {
            Self::Converted { rate, .. } => rate.as_ref(),
            _ => None,
        }
    }
}

/// Totals in the billing currency: sums of the converted lines. Each is `None`
/// when a line it includes has no rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiConvertedTotals {
    /// Everything billed: fees, overhead and API usage.
    pub billed: Option<AiFeeAmount>,
    /// Subscription fees, allocated shares and overhead together.
    pub fees: Option<AiFeeAmount>,
    /// The unallocated overhead within `fees`.
    pub overhead: Option<AiFeeAmount>,
    /// API usage.
    pub api: Option<AiFeeAmount>,
}

/// A month's bill in the billing currency, beside the original amounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiConvertedBill {
    /// One per line of the bill, in its order.
    pub lines: Vec<AiConvertedAmount>,
    /// One per subscription of the bill, in its order: the pro-rated fee.
    pub subscriptions: Vec<AiConvertedAmount>,
    pub totals: AiConvertedTotals,
    /// Every rate stored for the month, used or not, by currency.
    pub rates: Vec<AiExchangeRate>,
    /// Currencies the bill has non-zero amounts in without a rate.
    pub missing_rates: Vec<AiCurrency>,
    /// Currencies needed by the bill whose fetch is still in progress. A
    /// provisional stored rate remains usable while its replacement is pending.
    pub pending_rates: Vec<AiCurrency>,
}

/// Converts `amount` with the month's `rates`. Zero needs no rate.
fn convert_amount(
    amount: &AiFeeAmount,
    rates: &[AiExchangeRate],
) -> Result<AiConvertedAmount, AiBillingError> {
    let billing = AiCurrency::billing();
    if amount.currency == billing {
        return Ok(AiConvertedAmount::Converted {
            amount: amount.clone(),
            rate: None,
        });
    }
    let rate = rates.iter().find(|rate| rate.currency == amount.currency);
    if amount.hundredths == 0 {
        return Ok(AiConvertedAmount::Converted {
            amount: AiFeeAmount {
                hundredths: 0,
                currency: billing,
            },
            rate: rate.cloned(),
        });
    }
    match rate.and_then(|rate| Some((rate, rate.rate()?))) {
        Some((rate, value)) => Ok(AiConvertedAmount::Converted {
            amount: AiFeeAmount {
                hundredths: value
                    .convert(amount.hundredths)
                    .ok_or(AiBillingError::NumericRange)?,
                currency: billing,
            },
            rate: Some(rate.clone()),
        }),
        None => Ok(AiConvertedAmount::MissingRate {
            currency: amount.currency.clone(),
        }),
    }
}

/// Converts a month's bill to the billing currency at `rates`, the rates
/// stored for the bill's month; `pending` are currencies still being fetched.
/// See the module's conversion rules.
pub fn convert_bill(
    bill: &AiMonthBill,
    rates: &[AiExchangeRate],
    pending: &[AiCurrency],
) -> Result<AiConvertedBill, AiBillingError> {
    let rates: Vec<AiExchangeRate> = rates
        .iter()
        .filter(|rate| rate.month == bill.month && rate.rate().is_some())
        .cloned()
        .collect();
    let mut lines: Vec<AiConvertedAmount> = bill
        .lines
        .iter()
        .map(|line| match line.billable() {
            Some(amount) => convert_amount(&amount, &rates),
            None => Ok(AiConvertedAmount::UnknownAmount),
        })
        .collect::<Result<_, _>>()?;

    let mut subscriptions = Vec::with_capacity(bill.subscriptions.len());
    for month in &bill.subscriptions {
        let converted_fee = convert_amount(&month.prorated_fee, &rates)?;
        let subscription_id = month.subscription.id;
        let indices: Vec<usize> = bill
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                matches!(
                    line.charge,
                    AiBillingCharge::Subscription { subscription_id: id, .. } if id == subscription_id
                )
            })
            .map(|(index, _)| index)
            .collect();

        // Split the converted fee by the weights that split the original fee.
        // Overhead is one line holding the whole fee, converted above already.
        if let AiConvertedAmount::Converted { amount, rate } = &converted_fee {
            let shares = if indices.len() == month.weights.len() {
                allocate_largest_remainder(amount.hundredths, &month.weights)?
            } else {
                None
            };
            if let Some(shares) = shares {
                for (index, share) in indices.iter().zip(shares) {
                    lines[*index] = AiConvertedAmount::Converted {
                        amount: AiFeeAmount {
                            hundredths: share,
                            currency: amount.currency.clone(),
                        },
                        rate: rate.clone(),
                    };
                }
            }
        }
        subscriptions.push(converted_fee);
    }

    let totals = converted_totals(&bill.lines, &lines)?;
    let mut missing_rates: BTreeMap<String, AiCurrency> = BTreeMap::new();
    for converted in lines.iter().chain(&subscriptions) {
        if let AiConvertedAmount::MissingRate { currency } = converted {
            missing_rates.insert(currency.as_str().to_string(), currency.clone());
        }
    }
    let missing_rates: Vec<AiCurrency> = missing_rates.into_values().collect();
    let pending_rates = currencies_to_convert(bill)
        .iter()
        .filter(|currency| pending.contains(currency))
        .cloned()
        .collect();

    Ok(AiConvertedBill {
        lines,
        subscriptions,
        totals,
        rates,
        missing_rates,
        pending_rates,
    })
}

fn converted_totals(
    lines: &[AiBillingLine],
    converted: &[AiConvertedAmount],
) -> Result<AiConvertedTotals, AiBillingError> {
    /// A running sum that turns unknown once a line without a rate joins it.
    fn add(total: &mut Option<i64>, converted: &AiConvertedAmount) -> Result<(), AiBillingError> {
        match converted {
            AiConvertedAmount::Converted { amount, .. } => {
                if let Some(sum) = total {
                    *sum = sum
                        .checked_add(amount.hundredths)
                        .ok_or(AiBillingError::NumericRange)?;
                }
            }
            AiConvertedAmount::MissingRate { .. } => *total = None,
            // Unknown amounts are already left out of the original totals.
            AiConvertedAmount::UnknownAmount => {}
        }
        Ok(())
    }

    let (mut billed, mut fees, mut overhead, mut api) = (Some(0), Some(0), Some(0), Some(0));
    for (line, converted) in lines.iter().zip(converted) {
        add(&mut billed, converted)?;
        match line.charge {
            AiBillingCharge::Api => add(&mut api, converted)?,
            AiBillingCharge::Subscription { .. } => add(&mut fees, converted)?,
            AiBillingCharge::UnallocatedOverhead { .. } => {
                add(&mut fees, converted)?;
                add(&mut overhead, converted)?;
            }
        }
    }
    let amount = |hundredths: Option<i64>| {
        hundredths.map(|hundredths| AiFeeAmount {
            hundredths,
            currency: AiCurrency::billing(),
        })
    };

    Ok(AiConvertedTotals {
        billed: amount(billed),
        fees: amount(fees),
        overhead: amount(overhead),
        api: amount(api),
    })
}

#[cfg(test)]
mod tests;
