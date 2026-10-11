use async_trait::async_trait;
use time::Date;

use crate::domain::{models::AiDisplayRates, AiBillingError};

/// Rates for showing API-equivalent USD estimates in other currencies. They
/// reveal nobody's usage, so any signed-in user may read them.
#[async_trait]
pub trait AiDisplayRateService: Send + Sync + 'static {
    /// The display rates of a range of usage whose last local day is
    /// `through`, or of the current month without one: the rates its month
    /// bills at, or the month before's while it has none. Rates that are
    /// missing or due are fetched, waited for only briefly, and reported as
    /// pending.
    async fn display_rates(&self, through: Option<Date>) -> Result<AiDisplayRates, AiBillingError>;
}
