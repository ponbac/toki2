use bytes::Bytes;
use mime::Mime;
use mime_guess::MimeGuess;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use time::{Duration, OffsetDateTime};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Failed to download image: {0}")]
    DownloadError(#[from] reqwest::Error),
    #[error("Failed to write to cache: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid image format")]
    InvalidFormat,
    #[error("Cache directory not accessible")]
    CacheDirectoryError,
    #[error("Image too large: {size} bytes")]
    ImageTooLarge { size: u64 },
    #[error("Failed to parse metadata: {0}")]
    MetadataError(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct AvatarCacheService {
    cache_dir: PathBuf,
    http_client: Client,
    default_ttl: Duration,
    max_cache_size: u64,
    max_image_size: u64,
    api_base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageMetadata {
    original_url: String,
    downloaded_at: OffsetDateTime,
    expires_at: OffsetDateTime,
    file_size: u64,
    content_type: String,
    file_extension: String,
}

impl AvatarCacheService {
    pub fn new(
        cache_dir: PathBuf,
        default_ttl: Duration,
        max_cache_size: u64,
        max_image_size: u64,
        api_base_url: String,
    ) -> Self {
        Self {
            cache_dir,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            default_ttl,
            max_cache_size,
            max_image_size,
            api_base_url,
        }
    }

    pub async fn initialize(&self) -> Result<(), CacheError> {
        // Create cache directory structure
        let avatars_dir = self.cache_dir.join("avatars");
        fs::create_dir_all(&avatars_dir).await?;

        // Create .gitignore file
        let gitignore_path = self.cache_dir.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(&gitignore_path, "*\n!.gitignore\n").await?;
        }

        info!("Avatar cache initialized at: {}", self.cache_dir.display());
        Ok(())
    }

    pub async fn get_cached_avatar_url(
        &self,
        user_id: &str,
        original_url: &str,
    ) -> Result<String, CacheError> {
        let cache_key = self.generate_cache_key(user_id);
        let user_dir = self.cache_dir.join("avatars").join(&cache_key);
        let metadata_path = user_dir.join("metadata.json");

        // Check if cached image exists and is valid
        if metadata_path.exists() {
            match self.read_metadata(&metadata_path).await {
                Ok(metadata) => {
                    if metadata.original_url == original_url {
                        let image_path =
                            user_dir.join(&format!("avatar.{}", metadata.file_extension));
                        if image_path.exists() {
                            // Check if cache is still valid
                            if OffsetDateTime::now_utc() < metadata.expires_at {
                                return Ok(format!("{}/avatars/{}", self.api_base_url, cache_key));
                            } else {
                                // Cache is expired, trigger background refresh
                                tokio::spawn({
                                    let cache_service = self.clone();
                                    let user_id = user_id.to_string();
                                    let original_url = original_url.to_string();
                                    async move {
                                        if let Err(e) = cache_service
                                            .download_and_cache(&user_id, &original_url)
                                            .await
                                        {
                                            error!(
                                                "Failed to refresh avatar cache for user {}: {}",
                                                user_id, e
                                            );
                                        }
                                    }
                                });
                                // Return cached URL while refreshing
                                return Ok(format!("{}/avatars/{}", self.api_base_url, cache_key));
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read metadata for user {}: {}", user_id, e);
                }
            }
        }

        // Cache miss or invalid cache - download image
        self.download_and_cache(user_id, original_url).await?;
        Ok(format!("{}/avatars/{}", self.api_base_url, cache_key))
    }

    pub async fn download_and_cache(
        &self,
        user_id: &str,
        original_url: &str,
    ) -> Result<PathBuf, CacheError> {
        let cache_key = self.generate_cache_key(user_id);
        let user_dir = self.cache_dir.join("avatars").join(&cache_key);

        // Create user directory
        fs::create_dir_all(&user_dir).await?;

        // Download image
        let response = self.http_client.get(original_url).send().await?;

        if !response.status().is_success() {
            return Err(CacheError::DownloadError(reqwest::Error::from(
                response.error_for_status().unwrap_err(),
            )));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        // Check image size
        if content_length > self.max_image_size {
            return Err(CacheError::ImageTooLarge {
                size: content_length,
            });
        }

        let image_bytes = response.bytes().await?;

        // Validate image format
        let mime_type: Mime = content_type
            .parse()
            .map_err(|_| CacheError::InvalidFormat)?;

        if !mime_type.type_().as_str().starts_with("image") {
            return Err(CacheError::InvalidFormat);
        }

        // Determine file extension
        let file_extension = self.get_file_extension(&mime_type, original_url);
        let image_path = user_dir.join(&format!("avatar.{}", file_extension));

        // Write image to disk
        let mut file = fs::File::create(&image_path).await?;
        file.write_all(&image_bytes).await?;
        file.flush().await?;

        // Create metadata
        let now = OffsetDateTime::now_utc();
        let metadata = ImageMetadata {
            original_url: original_url.to_string(),
            downloaded_at: now,
            expires_at: now + self.default_ttl,
            file_size: image_bytes.len() as u64,
            content_type: content_type.to_string(),
            file_extension: file_extension.clone(),
        };

        let metadata_path = user_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json).await?;

        info!(
            "Cached avatar for user {} at {}",
            user_id,
            image_path.display()
        );
        Ok(image_path)
    }

    pub async fn get_cached_image(&self, user_hash: &str) -> Result<(Bytes, String), CacheError> {
        let user_dir = self.cache_dir.join("avatars").join(user_hash);
        let metadata_path = user_dir.join("metadata.json");

        if !metadata_path.exists() {
            return Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Avatar not found in cache",
            )));
        }

        let metadata = self.read_metadata(&metadata_path).await?;
        let image_path = user_dir.join(&format!("avatar.{}", metadata.file_extension));

        if !image_path.exists() {
            return Err(CacheError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Avatar image file not found",
            )));
        }

        let image_bytes = fs::read(&image_path).await?;
        Ok((Bytes::from(image_bytes), metadata.content_type))
    }

    pub async fn cleanup_expired(&self) -> Result<(), CacheError> {
        let avatars_dir = self.cache_dir.join("avatars");
        if !avatars_dir.exists() {
            return Ok(());
        }

        let mut entries = fs::read_dir(&avatars_dir).await?;
        let mut cleanup_count = 0;

        while let Some(entry) = entries.next_entry().await? {
            let user_dir = entry.path();
            if !user_dir.is_dir() {
                continue;
            }

            let metadata_path = user_dir.join("metadata.json");
            if metadata_path.exists() {
                match self.read_metadata(&metadata_path).await {
                    Ok(metadata) => {
                        // Check if cache is expired beyond grace period
                        let grace_period = Duration::days(7);
                        if OffsetDateTime::now_utc() > metadata.expires_at + grace_period {
                            if let Err(e) = fs::remove_dir_all(&user_dir).await {
                                error!(
                                    "Failed to remove expired cache directory {}: {}",
                                    user_dir.display(),
                                    e
                                );
                            } else {
                                cleanup_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to read metadata for cleanup in {}: {}",
                            user_dir.display(),
                            e
                        );
                        // Remove corrupted cache entries
                        if let Err(e) = fs::remove_dir_all(&user_dir).await {
                            error!(
                                "Failed to remove corrupted cache directory {}: {}",
                                user_dir.display(),
                                e
                            );
                        } else {
                            cleanup_count += 1;
                        }
                    }
                }
            } else {
                // Remove directories without metadata
                if let Err(e) = fs::remove_dir_all(&user_dir).await {
                    error!(
                        "Failed to remove invalid cache directory {}: {}",
                        user_dir.display(),
                        e
                    );
                } else {
                    cleanup_count += 1;
                }
            }
        }

        if cleanup_count > 0 {
            info!("Cleaned up {} expired avatar cache entries", cleanup_count);
        }

        Ok(())
    }

    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let cache_service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 60 * 60)); // 6 hours
            loop {
                interval.tick().await;
                if let Err(e) = cache_service.cleanup_expired().await {
                    error!("Avatar cache cleanup failed: {}", e);
                }
            }
        })
    }

    fn generate_cache_key(&self, user_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(user_id.as_bytes());
        let hash = hasher.finalize();
        format!("{:x}", hash)[..16].to_string() // Use first 16 chars of hash
    }

    fn get_file_extension(&self, mime_type: &Mime, url: &str) -> String {
        // First try to get extension from MIME type
        match mime_type.subtype().as_str() {
            "jpeg" => "jpg".to_string(),
            "png" => "png".to_string(),
            "gif" => "gif".to_string(),
            "webp" => "webp".to_string(),
            "svg+xml" => "svg".to_string(),
            _ => {
                // Fall back to guessing from URL
                MimeGuess::from_path(url)
                    .first()
                    .and_then(|mime| match mime.subtype().as_str() {
                        "jpeg" => Some("jpg".to_string()),
                        "png" => Some("png".to_string()),
                        "gif" => Some("gif".to_string()),
                        "webp" => Some("webp".to_string()),
                        "svg+xml" => Some("svg".to_string()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "jpg".to_string()) // Default to jpg
            }
        }
    }

    async fn read_metadata(&self, path: &PathBuf) -> Result<ImageMetadata, CacheError> {
        let content = fs::read_to_string(path).await?;
        let metadata: ImageMetadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }
}
