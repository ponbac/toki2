use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AiAdoptionCredit, AiAdoptionCredits, AiAdoptionPerson, AiAdoptionUsageRow,
        AiUsageDateRange, AiUsageTimeZone, AiUsdNanos, UserId,
    },
    AiBillingError,
};

/// What adoption reads of stored AI usage and a metered payer's imported
/// credits. Usage is never read finer than one local day per person and model:
/// adoption sees no hours or sessions. It reads every key, personal ones too.
#[async_trait]
pub trait AiAdoptionRepository: Send + Sync + 'static {
    /// Today's date in `time_zone`, by the store's clock.
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiBillingError>;

    /// Usage with local dates in `dates`, of `user_id` or everyone, summed per
    /// user, local day, tool, payer and model. With `credits`, the buckets its
    /// payer pays for (`COALESCE(billing_provider, provider)`) from its first
    /// instant on are summed apart, marked `credited`.
    async fn usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        credits: Option<&AiAdoptionCredits>,
        user_id: Option<&UserId>,
    ) -> Result<Vec<AiAdoptionUsageRow>, AiBillingError>;

    /// The credits `credits.payer` charged on UTC days within `dates`, from
    /// `credits.from` on, to accounts that belonged to a user on the day,
    /// summed per user and day. Usage the payer bills per repository is no
    /// one's.
    async fn user_credits(
        &self,
        credits: &AiAdoptionCredits,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiAdoptionCredit>, AiBillingError>;

    /// The credits `credits.payer` charged the whole organization per UTC day
    /// within `dates`, from `credits.from` on.
    async fn company_credits(
        &self,
        credits: &AiAdoptionCredits,
        dates: AiUsageDateRange,
    ) -> Result<Vec<(Date, AiUsdNanos)>, AiBillingError>;

    /// The last UTC day through which `credits.payer`'s credits are complete:
    /// the organization's, and those of every account that belongs to a user
    /// on `today`. An account that never completed a day holds it before
    /// `credits.from`, and `None` is returned then, or without any.
    async fn credits_complete_through(
        &self,
        credits: &AiAdoptionCredits,
        today: Date,
    ) -> Result<Option<Date>, AiBillingError>;

    /// Every user, whether they are linked to a time-tracking user, the local
    /// day in `time_zone` their first machine was first seen, and the seats
    /// they hold on `today` through accounts that are theirs that day.
    async fn people(
        &self,
        today: Date,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiAdoptionPerson>, AiBillingError>;
}
