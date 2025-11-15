use az_devops::Identity;
use futures::future::join_all;
use std::sync::Arc;
use tracing::{error, warn};

use crate::services::avatar_cache::{AvatarCacheService, CacheError};

#[derive(Debug, Clone)]
pub struct IdentityProcessor {
    avatar_cache: Arc<AvatarCacheService>,
}

impl IdentityProcessor {
    pub fn new(avatar_cache: Arc<AvatarCacheService>) -> Self {
        Self { avatar_cache }
    }

    pub async fn process_identity(&self, identity: Identity) -> Identity {
        let local_avatar_url = if let Some(ref original_url) = identity.avatar_url {
            match self
                .avatar_cache
                .get_cached_avatar_url(&identity.id, original_url)
                .await
            {
                Ok(local_url) => Some(local_url),
                Err(e) => {
                    // Log error and fall back to original URL
                    match e {
                        CacheError::DownloadError(_) => {
                            warn!("Failed to download avatar for user {}: {}", identity.id, e);
                        }
                        CacheError::InvalidFormat => {
                            warn!(
                                "Invalid avatar format for user {}: {}",
                                identity.id, original_url
                            );
                        }
                        CacheError::ImageTooLarge { size } => {
                            warn!(
                                "Avatar too large for user {} ({}KB): {}",
                                identity.id,
                                size / 1024,
                                original_url
                            );
                        }
                        _ => {
                            error!("Failed to cache avatar for user {}: {}", identity.id, e);
                        }
                    }
                    // Return original URL as fallback
                    identity.avatar_url.clone()
                }
            }
        } else {
            None
        };

        Identity {
            avatar_url: local_avatar_url,
            ..identity
        }
    }

    pub async fn process_identities(&self, identities: Vec<Identity>) -> Vec<Identity> {
        // Process identities concurrently with bounded parallelism
        let chunk_size = 10; // Process 10 identities at a time
        let mut results = Vec::with_capacity(identities.len());

        for chunk in identities.chunks(chunk_size) {
            let tasks: Vec<_> = chunk
                .iter()
                .cloned()
                .map(|identity| {
                    let processor = self.clone();
                    async move { processor.process_identity(identity).await }
                })
                .collect();

            let chunk_results = join_all(tasks).await;
            results.extend(chunk_results);
        }

        results
    }

    pub async fn process_single_identity(&self, identity: &Identity) -> Identity {
        self.process_identity(identity.clone()).await
    }
}
