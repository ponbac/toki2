use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AiMachineId, AiPricingBasis, AiProviderRepricing, AiUsageBucket, AiUsageTimeZone,
        AiUsageUpload, UserId,
    },
    AiUsageError,
};

#[async_trait]
pub trait AiUsageRepository: Send + Sync + 'static {
    /// Atomically registers or updates the machine for `user_id`, with the
    /// upload's pricing basis, when it has one, as the machine's current one
    /// (an upload without one keeps the machine's), and records the coverage of
    /// every provider the upload reports. For each replaced provider
    /// (`AiUsageUpload::replaced_providers`), records the upload's pricing,
    /// deletes the machine's buckets and hints inside the upload window, and
    /// inserts the upload's, each with the upload's pricing basis. It also
    /// logs, per replaced provider, the interval the upload proves complete:
    /// the window, ending no later than now. Other providers' stored usage and
    /// pricing are untouched. Returns the number of buckets stored.
    ///
    /// Fails with `MachineOwnedByAnotherUser`, and changes nothing, when another
    /// user registered the machine. Fails with `SupersededUpload`, and changes
    /// nothing, when the upload reports a snapshot older than the newest one
    /// already applied for the machine.
    async fn replace_window(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<u64, AiUsageError>;

    /// One entry per provider with buckets of `user_id` from `machine_id`,
    /// ordered by provider name: the earliest hour from `floor` on whose
    /// bucket's pricing basis is not `basis`, or none, and how many such
    /// buckets there are. Buckets without a basis count as another basis.
    async fn stale_pricing(
        &self,
        user_id: &UserId,
        machine_id: &AiMachineId,
        basis: &AiPricingBasis,
        floor: OffsetDateTime,
    ) -> Result<Vec<AiProviderRepricing>, AiUsageError>;

    /// In one transaction, sets the current pricing basis of `machine_id` to
    /// `basis` when `user_id` registered it, also when `buckets` is empty, and
    /// sets the cost, unpriced record count and `basis` of each stored bucket
    /// of `user_id` from `machine_id` that has the same key (hour, session,
    /// project, provider, model and payer), the same token counts and the same
    /// record count as one of `buckets`. Inserts and deletes nothing, and
    /// leaves the machine's other fields, its coverage and its hints alone.
    /// Returns how many stored buckets matched, including any whose values
    /// were equal already. Bucket keys must be unique.
    async fn reprice_buckets(
        &self,
        user_id: &UserId,
        machine_id: &AiMachineId,
        basis: &AiPricingBasis,
        buckets: &[&AiUsageBucket],
    ) -> Result<u64, AiUsageError>;

    /// When the first open month of the AI bill at or after the configured
    /// floor's month starts: the earlier of its UTC and its `time_zone`
    /// midnight. As `AiMonthCloseRepository::open_from`.
    async fn open_from(
        &self,
        time_zone: &AiUsageTimeZone,
        configured_floor: OffsetDateTime,
    ) -> Result<OffsetDateTime, AiUsageError>;
}
