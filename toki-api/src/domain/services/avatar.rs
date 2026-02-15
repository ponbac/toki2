use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;

use crate::domain::{
    models::{AvatarImage, AvatarOverride, UserId},
    ports::{
        inbound::AvatarService,
        outbound::{AvatarProcessor, AvatarRepository},
    },
    AvatarError,
};

const MAX_AVATAR_SIZE: usize = 5 * 1024 * 1024;

pub struct AvatarServiceImpl<R, P> {
    repository: Arc<R>,
    processor: Arc<P>,
    api_url: String,
}

impl<R, P> AvatarServiceImpl<R, P> {
    pub fn new(repository: Arc<R>, processor: Arc<P>, api_url: impl Into<String>) -> Self {
        Self {
            repository,
            processor,
            api_url: api_url.into(),
        }
    }

    fn build_avatar_url(&self, user_id: &UserId, updated_at: time::OffsetDateTime) -> String {
        let base_url = self.api_url.trim_end_matches('/');
        let fingerprint = updated_at.unix_timestamp_nanos() / 1_000_000;
        format!(
            "{base_url}/users/{}/avatar?v={fingerprint}",
            user_id.as_i32()
        )
    }
}

#[async_trait]
impl<R: AvatarRepository, P: AvatarProcessor> AvatarService for AvatarServiceImpl<R, P> {
    async fn get_avatar(&self, user_id: &UserId) -> Result<Option<AvatarImage>, AvatarError> {
        self.repository.get_avatar(user_id).await
    }

    async fn upload_avatar(
        &self,
        user_id: &UserId,
        image: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<(), AvatarError> {
        if image.len() > MAX_AVATAR_SIZE {
            return Err(AvatarError::PayloadTooLarge);
        }

        if let Some(content_type) = content_type.as_deref() {
            if !content_type.starts_with("image/") {
                return Err(AvatarError::UnsupportedMediaType);
            }
        }

        let processor = Arc::clone(&self.processor);
        let processed =
            tokio::task::spawn_blocking(move || processor.process(&image, content_type.as_deref()))
                .await
                .map_err(|err| {
                    AvatarError::Storage(format!("avatar processing task failed: {err}"))
                })??;

        self.repository.set_avatar(user_id, &processed).await
    }

    async fn delete_avatar(&self, user_id: &UserId) -> Result<(), AvatarError> {
        self.repository.delete_avatar(user_id).await
    }

    async fn get_avatar_url(&self, user_id: &UserId) -> Result<Option<String>, AvatarError> {
        let updated_at = self.repository.avatar_updated_at(user_id).await?;

        Ok(updated_at.map(|updated_at| self.build_avatar_url(user_id, updated_at)))
    }

    async fn resolve_overrides(
        &self,
        emails: &[String],
    ) -> Result<Vec<AvatarOverride>, AvatarError> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }

        let unique_emails = emails
            .iter()
            .map(|email| email.to_lowercase())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let records = self
            .repository
            .users_with_avatars_by_email(&unique_emails)
            .await?;

        let lookup = records
            .into_iter()
            .map(|record| {
                (
                    record.email,
                    self.build_avatar_url(&record.user_id, record.updated_at),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut overrides = Vec::new();
        let mut seen = HashSet::new();

        for email in emails {
            let normalized = email.to_lowercase();
            if seen.contains(&normalized) {
                continue;
            }

            if let Some(url) = lookup.get(&normalized) {
                overrides.push(AvatarOverride::new(normalized.clone(), url.clone()));
                seen.insert(normalized);
            }
        }

        Ok(overrides)
    }
}
