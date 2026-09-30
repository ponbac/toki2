use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AiBillingMonth, AiCurrency, AiExchangeRate, AiExchangeRateValue, AiProvidedRate,
        AiRateClock, AiUsageTimeZone, UserId,
    },
    AiBillingError, ExchangeRateError,
};

/// A source of exchange rates to the billing currency (`BILLING_CURRENCY`),
/// such as a central bank. Rates are averages of the provider's daily rates,
/// in units of the billing currency per one unit of `currency`.
#[async_trait]
pub trait ExchangeRateProvider: Send + Sync + 'static {
    /// The name stored as the source of the rates it provides, such as
    /// `riksbank`.
    fn name(&self) -> &'static str;

    /// Whether a daily rate for a day after `day` may have been published by
    /// `now`, by the provider's publication schedule.
    fn may_have_published_after(&self, day: Date, now: OffsetDateTime) -> bool;

    /// The final average of a month that is over. `None` when the provider has
    /// no such average (yet), or does not know the currency.
    async fn monthly_average(
        &self,
        currency: &AiCurrency,
        month: AiBillingMonth,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError>;

    /// The average of the daily rates published from `from` through `through`,
    /// both inclusive. `None` when no day in the range has a rate.
    async fn daily_average(
        &self,
        currency: &AiCurrency,
        from: Date,
        through: Date,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError>;
}

/// A fetched rate to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiFetchedRateWrite {
    pub month: AiBillingMonth,
    pub currency: AiCurrency,
    pub provided: AiProvidedRate,
    /// The provider's name, such as `riksbank`.
    pub provider: &'static str,
    pub provisional: bool,
}

/// Stored exchange rates, one per month and currency, each with its fetched
/// rate and an admin's override kept apart.
#[async_trait]
pub trait AiExchangeRateRepository: Send + Sync + 'static {
    /// The rates stored for `month`, by currency, with the local dates in
    /// `time_zone` they were fetched or set on.
    async fn month_rates(
        &self,
        month: AiBillingMonth,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiExchangeRate>, AiBillingError>;

    /// Stores a fetched rate, replacing a fetched provisional one but never a
    /// final one with a provisional one. An override is left as it is.
    async fn store_fetched(&self, rate: &AiFetchedRateWrite) -> Result<(), AiBillingError>;

    /// Sets an admin's override of a month's rate. The fetched rate is kept.
    async fn set_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        by: &UserId,
    ) -> Result<(), AiBillingError>;

    /// Removes an admin's override, if any; the fetched rate applies again.
    async fn remove_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<(), AiBillingError>;

    /// The store's clock: now, and today in `time_zone`.
    async fn clock(&self, time_zone: &AiUsageTimeZone) -> Result<AiRateClock, AiBillingError>;
}
