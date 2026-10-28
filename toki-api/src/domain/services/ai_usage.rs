use std::sync::Arc;

use async_trait::async_trait;
use time::{OffsetDateTime, UtcOffset};

use crate::domain::{
    models::{
        AiMachineId, AiPricingBasis, AiRepricing, AiRepricingReceipt, AiRepricingStatus,
        AiUsageIngestReceipt, AiUsageUpload, UserId,
    },
    ports::{inbound::AiUsageService, outbound::AiUsageRepository},
    AiUsageError,
};

pub struct AiUsageServiceImpl<R> {
    repository: Arc<R>,
    reprice_floor: OffsetDateTime,
}

impl<R> AiUsageServiceImpl<R> {
    /// `reprice_floor` is the earliest hour a reprice-only upload may change:
    /// costs before it stay as they are.
    pub fn new(repository: Arc<R>, reprice_floor: OffsetDateTime) -> Self {
        Self {
            repository,
            reprice_floor: reprice_floor.to_offset(UtcOffset::UTC),
        }
    }
}

#[async_trait]
impl<R: AiUsageRepository> AiUsageService for AiUsageServiceImpl<R> {
    async fn ingest(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<AiUsageIngestReceipt, AiUsageError> {
        let stored_buckets = self.repository.replace_window(user_id, upload).await?;

        Ok(AiUsageIngestReceipt { stored_buckets })
    }

    async fn repricing_status(
        &self,
        user_id: &UserId,
        machine_id: &AiMachineId,
        basis: &AiPricingBasis,
    ) -> Result<AiRepricingStatus, AiUsageError> {
        let providers = self
            .repository
            .stale_pricing(user_id, machine_id, basis, self.reprice_floor)
            .await?;

        Ok(AiRepricingStatus {
            floor: self.reprice_floor,
            providers,
        })
    }

    async fn reprice(
        &self,
        user_id: &UserId,
        repricing: &AiRepricing,
    ) -> Result<AiRepricingReceipt, AiUsageError> {
        let (repriced, before_floor) = repricing.split_at_floor(self.reprice_floor);
        // Called even without buckets from the floor on: the repricing's basis
        // becomes the machine's current one either way.
        let updated = self
            .repository
            .reprice_buckets(
                user_id,
                &repricing.machine_id(),
                repricing.basis(),
                &repriced,
            )
            .await?;
        let receipt = AiRepricingReceipt {
            updated,
            unmatched: (repriced.len() as u64).saturating_sub(updated),
            before_floor,
        };
        tracing::info!(
            machine_id = %repricing.machine_id(),
            client_version = repricing.client_version(),
            pricing_status = repricing.pricing().status.as_str(),
            pricing_basis = repricing.basis().as_str(),
            updated = receipt.updated,
            unmatched = receipt.unmatched,
            before_floor = receipt.before_floor,
            "Repriced stored AI usage"
        );

        Ok(receipt)
    }
}
