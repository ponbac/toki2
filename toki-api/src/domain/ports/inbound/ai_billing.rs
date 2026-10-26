use async_trait::async_trait;
use serde_json::Value;

use crate::domain::{
    models::{
        AiBillingDeveloper, AiBillingLinePart, AiBillingMonth, AiCloseReadiness, AiCloseWaiver,
        AiConvertedBill, AiCurrency, AiDeveloperMonth, AiExchangeRate, AiExchangeRateValue,
        AiMeteredCutover, AiMonthBill, AiMonthCloseId, AiMonthCloseStatus, AiMonthCompleteness,
        AiMonthOverview, AiMonthView, AiUsageTimeZone, UserId,
    },
    AiBillingError, AiMonthCloseError,
};

/// Renders a bill as its readers see it (JSON and CSV), so a closed month
/// serves what it served at close. Implemented by the HTTP adapter, which
/// owns the projection, and passed in by the close handler.
pub trait AiBillProjector: Send + Sync {
    fn project(&self, input: &AiProjectionInput<'_>) -> Result<AiProjectedBill, AiBillingError>;

    /// The readiness as the close endpoints serve it, which the close keeps.
    fn project_readiness(&self, readiness: &AiCloseReadiness) -> Value;
}

/// What a close projects.
pub struct AiProjectionInput<'a> {
    /// The open month as it closes, its adjustment lines included.
    pub overview: &'a AiMonthOverview,
    /// The overview's own lines as parts (`AiBillingLine::parts`).
    pub parts: &'a [AiBillingLinePart<'a>],
    /// The close being stored, which the CSV names.
    pub close_id: AiMonthCloseId,
    /// Each developer's bill part, as the developer page shows it.
    pub developer_bills: &'a [(UserId, AiMonthBill, AiConvertedBill)],
}

/// A projected bill.
#[derive(Debug, Clone, PartialEq)]
pub struct AiProjectedBill {
    /// `AI_BILL_PROJECTION_VERSION` of the projection that made it.
    pub version: i16,
    /// The overview without its lines, adjustment lines and close.
    pub bill_json: Value,
    /// Per user id, the developer page's bill part.
    pub developer_json: Value,
    /// One per part, in order.
    pub lines: Vec<Value>,
    /// One per adjustment line, in order.
    pub adjustment_lines: Vec<Value>,
    pub csv_header: Vec<String>,
    /// The parts' rows, then the adjustment lines', in that order: raw cells.
    pub csv_rows: Vec<Vec<String>>,
    /// Indices into `csv_rows` in the CSV's row order.
    pub csv_order: Vec<usize>,
}

/// Monthly AI usage billing for admins. Callers must hold admin power; the
/// service reads everyone's usage, but never finer than one local day.
#[async_trait]
pub trait AiBillingService: Send + Sync + 'static {
    /// The time zone whose calendar days and months billing uses.
    fn time_zone(&self) -> &AiUsageTimeZone;

    /// From which UTC month a metered payer's usage bills from its credits;
    /// `None` keeps every month on estimates.
    fn metered_cutover(&self) -> Option<AiMeteredCutover>;

    /// Everyone's bill for a month in the configured time zone, also in the
    /// billing currency at the month's exchange rates. Rates that are missing
    /// or due are fetched, waited for only briefly, and reported as pending.
    /// From the metered cutover on, the metered payer's lines come from its
    /// credits of the UTC month with the same key. An open month carries the
    /// adjustment lines of the closed months that post on it; a closed month
    /// is its snapshot, with its rates as stored now.
    async fn month_overview(&self, month: AiBillingMonth) -> Result<AiMonthView, AiBillingError>;

    /// One developer's bill for a month, with their usage per local day by
    /// provider, model, machine and project (the metered payer's per UTC day
    /// from the cutover on). Its metered lines are the developer's lines of
    /// the overview's allocation, so both agree to the cent; its adjustment
    /// lines are the developer's. A closed month's bill comes from its
    /// snapshot; the usage stays live. `UserNotFound` for an unknown user.
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

    /// Where a month stands with close: open, with its readiness once it is
    /// over, or closed; and its close history.
    async fn close_status(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError>;

    /// Closes a month's whole AI bill once it is over and every failed
    /// readiness check is waived: its snapshot, as `projector` renders it,
    /// is what it serves from then on. `NotReady` with the readiness
    /// otherwise. A metered month holds the payer's imports off from its
    /// first read to the store, and is refused (`ImportRunning`) while any
    /// import of the payer runs, which no waiver covers.
    async fn close_month(
        &self,
        month: AiBillingMonth,
        by: &UserId,
        waivers: Vec<AiCloseWaiver>,
        projector: &dyn AiBillProjector,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError>;

    /// Re-opens a closed month, keeping its close as history, so its bill is
    /// computed live again. `HeldByAdjustments` while an active close holds
    /// adjustments to it.
    async fn reopen_month(
        &self,
        month: AiBillingMonth,
        by: &UserId,
    ) -> Result<AiMonthCloseStatus, AiMonthCloseError>;
}
