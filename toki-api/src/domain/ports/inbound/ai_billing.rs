use async_trait::async_trait;

use crate::domain::{
    models::{
        AiBillingDeveloper, AiBillingMonth, AiCurrency, AiDeveloperMonth, AiExchangeRate,
        AiExchangeRateValue, AiMonthCompleteness, AiMonthOverview, AiUsageTimeZone, UserId,
    },
    AiBillingError,
};

/// Monthly AI usage billing for admins. Callers must hold admin power; the
/// service reads everyone's usage, but never finer than one local day.
#[async_trait]
pub trait AiBillingService: Send + Sync + 'static {
    /// The time zone whose calendar days and months billing uses.
    fn time_zone(&self) -> &AiUsageTimeZone;

    /// Everyone's bill for a month in the configured time zone, also in the
    /// billing currency at the month's exchange rates. Rates that are missing
    /// or due are fetched, waited for only briefly, and reported as pending.
    async fn month_overview(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthOverview, AiBillingError>;

    /// One developer's bill for a month, with their usage per local day by
    /// provider, model, machine and project. `UserNotFound` for an unknown user.
    async fn developer_month(
        &self,
        month: AiBillingMonth,
        user_id: UserId,
    ) -> Result<AiDeveloperMonth, AiBillingError>;

    /// Whether each developer's machines have uploaded all of a month's usage.
    async fn completeness(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthCompleteness, AiBillingError>;

    /// Every user, by name, such as to declare a subscription for.
    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiBillingError>;

    /// Overrides a month's exchange rate for `currency`, keeping the fetched
    /// rate beneath it. A fetch never replaces it. `Invalid` for the billing
    /// currency.
    async fn override_exchange_rate(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        by: &UserId,
    ) -> Result<AiExchangeRate, AiBillingError>;

    /// Removes an override, so the stored fetched rate applies again, fetched
    /// only if missing or due: the rate the month now bills at, or `None`
    /// without one. `Invalid` for the billing currency.
    async fn reset_exchange_rate(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<Option<AiExchangeRate>, AiBillingError>;
}
