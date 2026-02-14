use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{AvatarIdentityRecord, AvatarImage, UserId},
    AvatarError,
};

#[async_trait]
pub trait AvatarRepository: Send + Sync + 'static {
    async fn get_avatar(&self, user_id: &UserId) -> Result<Option<AvatarImage>, AvatarError>;

    async fn set_avatar(&self, user_id: &UserId, image: &AvatarImage) -> Result<(), AvatarError>;

    async fn delete_avatar(&self, user_id: &UserId) -> Result<(), AvatarError>;

    async fn avatar_updated_at(
        &self,
        user_id: &UserId,
    ) -> Result<Option<OffsetDateTime>, AvatarError>;

    async fn users_with_avatars_by_email(
        &self,
        emails: &[String],
    ) -> Result<Vec<AvatarIdentityRecord>, AvatarError>;
}
