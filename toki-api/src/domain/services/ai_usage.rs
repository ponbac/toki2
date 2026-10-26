use std::sync::Arc;

use async_trait::async_trait;
use time::{OffsetDateTime, UtcOffset};

use crate::domain::{
    models::{
        reprice_floor, AiMachineId, AiPricingBasis, AiRepricing, AiRepricingReceipt,
        AiRepricingStatus, AiUsageIngestReceipt, AiUsageTimeZone, AiUsageUpload, UserId,
    },
    ports::{inbound::AiUsageService, outbound::AiUsageRepository},
    AiUsageError,
};

pub struct AiUsageServiceImpl<R> {
    repository: Arc<R>,
    reprice_floor: OffsetDateTime,
    time_zone: AiUsageTimeZone,
}

impl<R> AiUsageServiceImpl<R> {
    /// `reprice_floor` is the earliest hour a reprice-only upload may change:
    /// costs before it stay as they are. Closing the first open month of the
    /// AI bill raises it further (`floor`); `time_zone` is the one billing
    /// months follow.
    pub fn new(
        repository: Arc<R>,
        reprice_floor: OffsetDateTime,
        time_zone: AiUsageTimeZone,
    ) -> Self {
        Self {
            repository,
            reprice_floor: reprice_floor.to_offset(UtcOffset::UTC),
            time_zone,
        }
    }
}

impl<R: AiUsageRepository> AiUsageServiceImpl<R> {
    /// The earliest hour a reprice may change (`reprice_floor`): the
    /// configured floor, or the start of the first open month at or after
    /// its month, whichever is later. Every month from the configured floor's
    /// month up to it is closed. A closed month after the first open one
    /// stays repriceable: a reprice of it, like one of a closed month's last
    /// hours in its other basis or one that races a close, only changes the
    /// closed month's recompute, which its next open month nets as an
    /// adjustment.
    async fn floor(&self) -> Result<OffsetDateTime, AiUsageError> {
        let open_from = self
            .repository
            .open_from(&self.time_zone, self.reprice_floor)
            .await?;
        Ok(reprice_floor(self.reprice_floor, open_from))
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
        let floor = self.floor().await?;
        let providers = self
            .repository
            .stale_pricing(user_id, machine_id, basis, floor)
            .await?;

        Ok(AiRepricingStatus { floor, providers })
    }

    async fn reprice(
        &self,
        user_id: &UserId,
        repricing: &AiRepricing,
    ) -> Result<AiRepricingReceipt, AiUsageError> {
        let (repriced, before_floor) = repricing.split_at_floor(self.floor().await?);
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
