use async_trait::async_trait;

use crate::domain::{
    models::{AvatarImage, AvatarOverride, UserId},
    AvatarError,
};

#[async_trait]
pub trait AvatarService: Send + Sync + 'static {
    async fn get_avatar(&self, user_id: &UserId) -> Result<Option<AvatarImage>, AvatarError>;

    async fn upload_avatar(
        &self,
        user_id: &UserId,
        image: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<(), AvatarError>;

    async fn delete_avatar(&self, user_id: &UserId) -> Result<(), AvatarError>;

    async fn get_avatar_url(&self, user_id: &UserId) -> Result<Option<String>, AvatarError>;

    async fn resolve_overrides(
        &self,
        emails: &[String],
    ) -> Result<Vec<AvatarOverride>, AvatarError>;
}
