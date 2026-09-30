use async_trait::async_trait;

use crate::domain::{
    models::{
        AiUsageDateRange, AiUsagePeriod, AiUsagePeriodTotals, AiUsageTimeZone, AiUsageUpload,
        TimeTrackingCompany, UserId,
    },
    AiUsageError,
};

#[async_trait]
pub trait AiUsageRepository: Send + Sync + 'static {
    /// Atomically registers or updates the machine for `user_id` and records the
    /// coverage of every provider the upload reports. For each replaced provider
    /// (`AiUsageUpload::replaced_providers`), records the upload's pricing,
    /// deletes the machine's buckets and hints inside the upload window, and
    /// inserts the upload's. It also logs, per replaced provider, the interval
    /// the upload proves complete: the window, ending no later than now. Other
    /// providers' stored usage and pricing are untouched. Returns the number of
    /// buckets stored.
    ///
    /// Fails with `MachineOwnedByAnotherUser`, and changes nothing, when another
    /// user registered the machine.
    async fn replace_window(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<u64, AiUsageError>;

    /// Sums the user's buckets across machines per project and local day or
    /// month in `time_zone`, for local dates in `dates`. A bucket belongs to the
    /// local date of its start instant. Each row reports the dates it covers,
    /// clipped to `dates`, so a partly covered month is visibly partial, and the
    /// project its key is mapped to now, if the mapping names a project of
    /// `mapping_company`. Unknown costs stay visible as unpriced counts.
    async fn period_totals(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        period: AiUsagePeriod,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
    ) -> Result<Vec<AiUsagePeriodTotals>, AiUsageError>;
}
