# Avatar Image Cache Implementation Plan

## Problem Statement

The current `Identity` struct contains an `avatar_url` field that points directly to Azure DevOps images. When this URL is sent to the client, the client cannot access the image without proper authentication cookies, resulting in broken avatar displays in the UI.

## Solution Overview

Implement a local image cache system that:

1. Downloads avatar images from Azure DevOps using authenticated requests
2. Stores them locally with a file-based cache
3. Serves cached images through a dedicated endpoint
4. Replaces Azure URLs with local URLs before sending data to clients

## Architecture Components

### 1. Image Cache Service

Create a new service module `toki-api/src/services/avatar_cache.rs` with the following responsibilities:

- **Download Management**: Fetch images from Azure URLs using authenticated HTTP clients
- **Storage Management**: Save images to local filesystem with organized structure
- **Cache Validation**: Check cache freshness and handle expiration
- **URL Generation**: Create local URLs for cached images

### 2. Storage Structure

```
./avatar_cache/
├── avatars/
│   ├── {user_id_hash}/
│   │   ├── avatar.jpg
│   │   └── metadata.json
│   └── ...
└── .gitignore
```

**Storage Strategy:**

- Hash user IDs to create directory names (privacy + collision avoidance)
- Store original image with detected file extension
- Include metadata file with download timestamp, original URL, and expiration info
- Use `.gitignore` to exclude cache from version control

### 3. Cache Management

**TTL Strategy:**

- Default cache TTL: 24 hours (configurable)
- Stale cache grace period: 7 days (serve stale while refreshing)
- Background refresh for frequently accessed images

**Cleanup Strategy:**

- Periodic cleanup task to remove expired images
- LRU eviction when cache size exceeds configured limit
- Graceful handling of partial downloads and corrupted files

## Implementation Details

### Phase 1: Core Cache Service

#### 1.1 Create Avatar Cache Service

```rust
// toki-api/src/services/avatar_cache.rs
use std::path::PathBuf;
use std::time::Duration;
use reqwest::Client;
use tokio::fs;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone)]
pub struct AvatarCacheService {
    cache_dir: PathBuf,
    http_client: Client,
    default_ttl: Duration,
    max_cache_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageMetadata {
    original_url: String,
    downloaded_at: time::OffsetDateTime,
    expires_at: time::OffsetDateTime,
    file_size: u64,
    content_type: String,
}

impl AvatarCacheService {
    pub fn new(cache_dir: PathBuf, default_ttl: Duration) -> Self;
    pub async fn get_cached_avatar_url(&self, user_id: &str, original_url: &str) -> Result<String, CacheError>;
    pub async fn download_and_cache(&self, user_id: &str, original_url: &str) -> Result<PathBuf, CacheError>;
    pub async fn cleanup_expired(&self) -> Result<(), CacheError>;
    fn generate_cache_key(&self, user_id: &str) -> String;
}
```

#### 1.2 Add Cache Service to AppState

```rust
// toki-api/src/app_state.rs
use crate::services::avatar_cache::AvatarCacheService;

pub struct AppState {
    // ... existing fields ...
    pub avatar_cache: Arc<AvatarCacheService>,
}
```

#### 1.3 Create Image Serving Route

```rust
// toki-api/src/routes/avatars.rs
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/avatar/:user_hash", get(serve_avatar))
}

async fn serve_avatar(
    State(app_state): State<AppState>,
    Path(user_hash): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    // Serve cached avatar image with proper headers
}
```

### Phase 2: Integration with Identity System

#### 2.1 Modify Identity Conversion

Update the `From<IdentityRef>` implementation to use cached URLs:

```rust
// az-devops/src/models/identity.rs
impl From<IdentityRef> for Identity {
    fn from(identity: IdentityRef) -> Self {
        let avatar_url = identity
            .graph_subject_base
            .links
            .unwrap()
            .get("avatar")
            .map(|obj| {
                Value::to_string(obj.get("href").unwrap())
                    .trim_matches('"')
                    .to_string()
            });

        Self {
            id: identity.id,
            display_name: identity.graph_subject_base.display_name.unwrap(),
            unique_name: identity.unique_name.unwrap(),
            avatar_url, // Keep original URL for now, transform later
        }
    }
}
```

#### 2.2 Create Identity Processing Service

```rust
// toki-api/src/services/identity_processor.rs
use az_devops::Identity;
use crate::services::avatar_cache::AvatarCacheService;

pub struct IdentityProcessor {
    avatar_cache: Arc<AvatarCacheService>,
    api_base_url: String,
}

impl IdentityProcessor {
    pub async fn process_identity(&self, identity: Identity) -> Identity {
        let local_avatar_url = if let Some(ref original_url) = identity.avatar_url {
            match self.avatar_cache.get_cached_avatar_url(&identity.id, original_url).await {
                Ok(local_url) => Some(local_url),
                Err(_) => {
                    // Log error and fall back to original URL or None
                    tracing::warn!("Failed to cache avatar for user {}", identity.id);
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
        let mut tasks = Vec::new();
        for identity in identities {
            tasks.push(self.process_identity(identity));
        }

        futures::future::join_all(tasks).await
    }
}
```

### Phase 3: Background Processing

#### 3.1 Integrate with RepoDiffer

Modify `RepoDiffer` to process avatars when updating identities:

```rust
// toki-api/src/domain/repo_differ.rs
impl RepoDiffer {
    async fn update_identities(&self) -> Result<(), RepoDifferError> {
        let identities = self
            .az_client
            .get_git_identities()
            .await
            .map_err(|_| RepoDifferError::Identities)?;

        // Process identities to cache avatars
        let identity_processor = IdentityProcessor::new(
            self.avatar_cache.clone(),
            self.api_base_url.clone(),
        );
        let processed_identities = identity_processor.process_identities(identities).await;

        let mut cached_identities = self.identities.write().await;
        cached_identities.update(processed_identities);

        Ok(())
    }
}
```

#### 3.2 Background Cleanup Task

```rust
// toki-api/src/services/avatar_cache.rs
impl AvatarCacheService {
    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let cache_service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_hours(6));
            loop {
                interval.tick().await;
                if let Err(e) = cache_service.cleanup_expired().await {
                    tracing::error!("Avatar cache cleanup failed: {}", e);
                }
            }
        })
    }
}
```

### Phase 4: Configuration and Environment

#### 4.1 Add Configuration Settings

```rust
// toki-api/src/config.rs
#[derive(Deserialize, Clone)]
pub struct AvatarCacheSettings {
    pub cache_dir: String,
    pub ttl_hours: u64,
    pub max_cache_size_mb: u64,
    pub max_concurrent_downloads: usize,
}

pub struct Settings {
    // ... existing fields ...
    pub avatar_cache: AvatarCacheSettings,
}
```

#### 4.2 Environment Variables

```env
# .env.local
AVATAR_CACHE_DIR=./avatar_cache
AVATAR_CACHE_TTL_HOURS=24
AVATAR_CACHE_MAX_SIZE_MB=500
AVATAR_CACHE_MAX_CONCURRENT_DOWNLOADS=10
```

## Error Handling Strategy

### Error Types

```rust
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
}
```

### Fallback Strategy

1. **Download Failure**: Log error, return original URL
2. **Cache Miss**: Download asynchronously, serve original URL temporarily
3. **Corruption**: Delete corrupted cache, re-download
4. **Disk Space**: Implement LRU eviction

## Security Considerations

### File System Security

- Validate file paths to prevent directory traversal
- Use secure file permissions (600 for images, 700 for directories)
- Implement file size limits to prevent DoS attacks
- Sanitize file names and extensions

### HTTP Security

- Use timeout for downloads (30 seconds)
- Implement rate limiting for download requests
- Validate image content types
- Set maximum image size limits (e.g., 5MB)

### Privacy

- Hash user IDs for directory names
- Don't log full URLs in production
- Implement secure cleanup of temporary files

## Testing Strategy

### Unit Tests

- Cache hit/miss scenarios
- URL generation and validation
- Cleanup logic
- Error handling paths

### Integration Tests

- End-to-end avatar serving
- Cache expiration behavior
- Background processing
- Database integration

### Performance Tests

- Concurrent download handling
- Cache size scaling
- Memory usage under load
- Disk I/O performance

## Monitoring and Observability

### Metrics

- Cache hit/miss ratios
- Download success/failure rates
- Cache size and cleanup frequency
- Response times for avatar serving

### Logging

- Download attempts and results
- Cache cleanup activities
- Error conditions and recovery
- Performance bottlenecks

## Deployment Considerations

### File System

- Ensure cache directory is writable
- Configure proper backup exclusions
- Set up log rotation for cache logs
- Monitor disk space usage

### Container Deployment

- Mount cache directory as persistent volume
- Configure proper resource limits
- Set up health checks for cache service
- Handle container restart scenarios

## Migration Strategy

### Phase 1: Soft Launch

- Deploy cache service but keep original URLs
- Test cache functionality with a subset of users
- Monitor performance and error rates

### Phase 2: Gradual Rollout

- Replace URLs for new identities only
- Monitor cache hit rates and performance
- Implement fallback mechanisms

### Phase 3: Full Migration

- Process all existing identities
- Remove original URL fallbacks
- Optimize cache performance based on usage patterns

## Future Enhancements

### Image Optimization

- Implement image resizing for different use cases
- Add WebP format support for better compression
- Implement progressive loading for large images

### Advanced Caching

- Add CDN integration for global deployment
- Implement multi-tier caching (memory + disk)
- Add cache warming for frequently accessed images

### Analytics

- Track most accessed avatars
- Implement cache efficiency metrics
- Add user behavior analytics for avatar usage

## Conclusion

This implementation plan provides a comprehensive solution for caching avatar images locally while maintaining good performance, security, and maintainability. The phased approach allows for gradual rollout and testing, while the modular design enables future enhancements and optimizations.

The key benefits of this approach:

- **Solves Authentication Issue**: Clients can access cached images without Azure cookies
- **Improves Performance**: Local serving reduces latency and external dependencies
- **Enhances Privacy**: User data stays within the application boundary
- **Scalable Design**: Can handle growing user bases and image volumes
- **Maintainable Code**: Clear separation of concerns and comprehensive error handling
