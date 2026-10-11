use async_trait::async_trait;

use crate::domain::{
    models::{
        AiUsageDateRange, AiUsageIngestReceipt, AiUsagePeriod, AiUsagePeriodTotals, AiUsageUpload,
        UserId,
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

    /// Sums the user's usage across machines per project and configured-zone
    /// day or month. Each row reports the dates it covers, clipped to `dates`,
    /// and the project its key is mapped to when read.
    async fn period_totals(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        period: AiUsagePeriod,
    ) -> Result<Vec<AiUsagePeriodTotals>, AiUsageError>;
}
