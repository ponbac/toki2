use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{AiUsageIngestReceipt, AiUsageUpload, UserId},
    ports::{inbound::AiUsageService, outbound::AiUsageRepository},
    AiUsageError,
};

pub struct AiUsageServiceImpl<R> {
    repository: Arc<R>,
}

impl<R> AiUsageServiceImpl<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
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
}
