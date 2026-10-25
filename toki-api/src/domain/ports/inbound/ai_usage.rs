use async_trait::async_trait;

use crate::domain::{
    models::{
        AiMachineId, AiPricingBasis, AiRepricing, AiRepricingReceipt, AiRepricingStatus,
        AiUsageIngestReceipt, AiUsageUpload, UserId,
    },
    AiUsageError,
};

#[async_trait]
pub trait AiUsageService: Send + Sync + 'static {
    /// Stores an upload for `user_id`. The first upload registers the machine to
    /// the user; an upload for a machine registered to another user is rejected.
    async fn ingest(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<AiUsageIngestReceipt, AiUsageError>;

    /// Where the user's stored usage from `machine_id`, from the reprice floor
    /// on, was priced with another basis than `basis`, per provider. A machine
    /// the user has not registered has no providers.
    async fn repricing_status(
        &self,
        user_id: &UserId,
        machine_id: &AiMachineId,
        basis: &AiPricingBasis,
    ) -> Result<AiRepricingStatus, AiUsageError>;

    /// Gives the user's stored buckets from the machine that `repricing`
    /// reproduces exactly, from the reprice floor on, its costs and basis, and
    /// records the basis as the machine's current one, also when no bucket
    /// lies from the floor on. Never inserts or deletes usage, and changes
    /// nothing else. A machine the user has not registered matches no bucket
    /// and keeps its basis.
    async fn reprice(
        &self,
        user_id: &UserId,
        repricing: &AiRepricing,
    ) -> Result<AiRepricingReceipt, AiUsageError>;
}
