use async_trait::async_trait;

use crate::domain::{
    models::{AiUsageUpload, UserId},
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
    /// user registered the machine. Fails with `SupersededUpload`, and changes
    /// nothing, when the upload reports a snapshot older than the newest one
    /// already applied for the machine.
    async fn replace_window(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<u64, AiUsageError>;
}
