use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{
        AiUsageDateRange, AiUsageIngestReceipt, AiUsagePeriod, AiUsagePeriodTotals,
        AiUsageTimeZone, AiUsageUpload, UserId,
    },
    ports::{inbound::AiUsageService, outbound::AiUsageRepository},
    AiUsageError,
};

pub struct AiUsageServiceImpl<R> {
    repository: Arc<R>,
    time_zone: AiUsageTimeZone,
}

impl<R> AiUsageServiceImpl<R> {
    /// `time_zone` defines usage days and billing months.
    pub fn new(repository: Arc<R>, time_zone: AiUsageTimeZone) -> Self {
        Self {
            repository,
            time_zone,
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

    async fn period_totals(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        period: AiUsagePeriod,
    ) -> Result<Vec<AiUsagePeriodTotals>, AiUsageError> {
        self.repository
            .period_totals(user_id, dates, period, &self.time_zone)
            .await
    }
}
