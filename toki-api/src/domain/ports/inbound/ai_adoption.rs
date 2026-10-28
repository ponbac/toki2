use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{AiAdoption, AiAdoptionModel, AiUsageDateRange, UserId},
    AiBillingError,
};

/// How much AI each person and the company use, for admins. Reads stop at one
/// local day per person and tool; completions are left out.
#[async_trait]
pub trait AiAdoptionService: Send + Sync + 'static {
    /// Everyone's use from `from` through `to`, local days, by default the
    /// first day of the month six months back through today.
    async fn adoption(
        &self,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<AiAdoption, AiBillingError>;

    /// One person's models from `from` through `to`, by default as `adoption`,
    /// largest first by records.
    async fn models(
        &self,
        user_id: &UserId,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<(AiUsageDateRange, Vec<AiAdoptionModel>), AiBillingError>;
}
