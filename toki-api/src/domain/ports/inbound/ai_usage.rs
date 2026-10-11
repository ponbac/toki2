use async_trait::async_trait;

use crate::domain::{
    models::{AiUsageIngestReceipt, AiUsageUpload, UserId},
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
}
