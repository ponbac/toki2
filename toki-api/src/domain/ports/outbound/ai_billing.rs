use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AiBillingDeveloper, AiBillingMachine, AiCoverageInterval, AiDailyUsage,
        AiDeveloperDayUsage, AiLocalDay, AiMachineId, AiMeteredCutover, AiOldBasisUsage,
        AiPersonalUsage, AiUsageDateRange, AiUsageTimeZone, TimeTrackingCompany, UserId,
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
    /// otherwise the usage is Unassigned, summed apart by kind: `unattributed`
    /// usage, and the usage of every other key together, whose names are not
    /// returned. The kinds' costs are rounded as the one group they bill as,
    /// so they sum to it exactly. Usage of a key its user marked personal is
    /// left out. A bucket belongs to the local date its hour starts on. This
    /// is what bills, so every page bills from the same sums. With `metered`,
    /// the buckets its payer pays for (`COALESCE(billing_provider,
    /// provider)`) from its first instant on are left out: they bill from the
    /// payer's credits per UTC month.
    async fn daily_usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        user_id: Option<&UserId>,
        metered: Option<&AiMeteredCutover>,
    ) -> Result<Vec<AiDailyUsage>, AiBillingError>;

    /// One user's usage in `dates`, summed per local day, provider, model,
    /// machine and project key, with each key's project as for `daily_usage`.
    /// Keys without a mapping are summed into one group with no key, so their
    /// names are never read; `unattributed` keeps its own group. Usage of the
    /// keys the user marked personal is left out, as it is from the bill.
    /// For display only: it sums finer groups than `daily_usage`. With
    /// `metered`, the local rows leave its buckets out as `daily_usage` does,
    /// and those buckets come back on the UTC days of `dates` instead, marked
    /// `metered`, after the local rows of the same date.
    async fn developer_usage(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        metered: Option<&AiMeteredCutover>,
    ) -> Result<Vec<AiDeveloperDayUsage>, AiBillingError>;

    /// What `daily_usage` leaves out: usage in `dates` of the keys each user
    /// marked personal, of `user_id` or everyone, summed per user, by user.
    /// Users without any are absent. With `metered`, its buckets are left
    /// out: their personal usage bills as a metered line.
    async fn personal_usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        user_id: Option<&UserId>,
        metered: Option<&AiMeteredCutover>,
    ) -> Result<Vec<AiPersonalUsage>, AiBillingError>;

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

    /// Every machine's logged sync intervals that overlap `[from, until)`,
    /// for every user.
    async fn coverage_intervals(
        &self,
        from: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<Vec<(AiMachineId, AiCoverageInterval)>, AiBillingError>;

    /// The buckets `daily_usage` reads for `dates` (the same local days,
    /// metered exclusion and personal keys left out), from `floor` on, whose
    /// pricing basis is none or not their machine's current one, per user,
    /// machine and provider, by user name, machine label and provider.
    async fn old_basis_usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        metered: Option<&AiMeteredCutover>,
        floor: OffsetDateTime,
    ) -> Result<Vec<AiOldBasisUsage>, AiBillingError>;
}
