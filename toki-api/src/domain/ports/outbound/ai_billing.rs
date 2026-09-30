use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AiBillingDeveloper, AiBillingMachine, AiDailyUsage, AiDeveloperDayUsage, AiLocalDay,
        AiUsageDateRange, AiUsageTimeZone, TimeTrackingCompany, UserId,
    },
    AiBillingError,
};

/// What billing reads of stored AI usage. Usage is never read finer than one
/// local day: billing sees no hours or sessions.
#[async_trait]
pub trait AiBillingRepository: Send + Sync + 'static {
    /// Usage in `dates`, local days in `time_zone`, of `user_id` or everyone,
    /// summed per user, provider, day and the project its key resolves to now
    /// (`AiMappingResolver` with `mapping_company` as the configured company);
    /// otherwise the usage is Unassigned. A bucket belongs to the local date its hour starts on. This
    /// is what bills, so every page bills from the same sums.
    async fn daily_usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        user_id: Option<&UserId>,
    ) -> Result<Vec<AiDailyUsage>, AiBillingError>;

    /// One user's usage in `dates`, summed per local day, provider, model,
    /// machine and project key, with each key's project as for `daily_usage`.
    /// For display only: it sums finer groups than `daily_usage`.
    async fn developer_usage(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
    ) -> Result<Vec<AiDeveloperDayUsage>, AiBillingError>;

    /// The local days of `dates` in `time_zone`, with the instants they span.
    async fn local_days(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiLocalDay>, AiBillingError>;

    /// The machines of `user_id`, or of everyone, as completeness sees them for
    /// `dates`, local days in `time_zone`: their last sync, the providers they
    /// stored usage of in `dates`, each provider's latest report, and the
    /// logged sync intervals that overlap `dates`. Ordered by user and label.
    async fn machines(
        &self,
        user_id: Option<&UserId>,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiBillingMachine>, AiBillingError>;

    /// Every user, by name.
    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiBillingError>;

    /// Today's date in `time_zone`, by the store's clock.
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiBillingError>;
}
