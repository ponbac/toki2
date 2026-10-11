//! Showing API-equivalent estimates in a currency other than USD.
//!
//! Estimates are priced in USD. The personal usage view can show them in the
//! billing currency, or another display currency, instead. Everything here is
//! pure; the display rate service loads the rates.
//!
//! - **The rates a month bills at.** A display rate is built on the month's
//!   exchange rates (`AiExchangeRate::rate`), an admin's override included, so
//!   an estimate reads the same here as on the month's bill.
//! - **One month for a whole range**: the month of the range's last day, or
//!   the current month when the range ends later. A month that has not begun
//!   has no rate.
//! - **The month before** stands in while the month has no USD rate, as on its
//!   first days, before a daily rate is published.
//! - **For reading, not billing.** A display rate scales estimates on screen.
//!   Billing converts each line by its own rules (`convert_bill`).

use time::Date;

use super::{AiBillingMonth, AiCurrency, AiExchangeRate, AiExchangeRateValue};

/// The currencies estimates can be shown in besides USD and the billing
/// currency.
const OTHER_DISPLAY_CURRENCIES: [&str; 1] = ["EUR"];

/// What one US dollar of an estimate is shown as in `currency`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDisplayRate {
    pub currency: AiCurrency,
    /// Units of `currency` per US dollar.
    pub per_usd: AiExchangeRateValue,
    /// Built on an average of the days published so far, which later days
    /// change.
    pub provisional: bool,
    /// Built on a rate an admin set.
    pub overridden: bool,
}

/// The display rates of a range of usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDisplayRates {
    /// The month whose rates these are.
    pub month: AiBillingMonth,
    /// The billing currency first, then the other display currencies that
    /// have a rate. Empty without a USD rate: nothing can be converted.
    pub rates: Vec<AiDisplayRate>,
    /// A fetch of the rates is still in progress; they may change shortly.
    pub pending: bool,
}

/// The month whose rates show a range of usage ending on `through`, or the
/// current month's usage without one.
pub fn display_rate_month(today: Date, through: Option<Date>) -> Option<AiBillingMonth> {
    AiBillingMonth::containing(through.map_or(today, |day| day.min(today)))
}

fn other_display_currencies() -> impl Iterator<Item = AiCurrency> {
    OTHER_DISPLAY_CURRENCIES
        .iter()
        .filter_map(|code| AiCurrency::parse(code).ok())
}

/// The currencies whose exchange rates the display rates are built on.
pub fn display_rate_currencies() -> Vec<AiCurrency> {
    std::iter::once(AiCurrency::usd())
        .chain(other_display_currencies())
        .collect()
}

/// The display rates that `rates` give for `month`. Empty without a USD rate;
/// another display currency without a rate is left out.
pub fn display_rates(month: AiBillingMonth, rates: &[AiExchangeRate]) -> Vec<AiDisplayRate> {
    let stored = |currency: &AiCurrency| {
        rates
            .iter()
            .find(|rate| rate.month == month && &rate.currency == currency)
            .and_then(|rate| Some((rate, rate.rate()?)))
    };
    let Some((usd, usd_rate)) = stored(&AiCurrency::usd()) else {
        return Vec::new();
    };

    let mut display = vec![AiDisplayRate {
        currency: AiCurrency::billing(),
        per_usd: usd_rate,
        provisional: usd.is_provisional(),
        overridden: usd.is_override(),
    }];
    display.extend(other_display_currencies().filter_map(|currency| {
        let (other, other_rate) = stored(&currency)?;
        Some(AiDisplayRate {
            currency,
            per_usd: usd_rate.per(other_rate)?,
            provisional: usd.is_provisional() || other.is_provisional(),
            overridden: usd.is_override() || other.is_override(),
        })
    }));
    display
}

#[cfg(test)]
mod tests;
