use async_trait::async_trait;
use sqlx::PgPool;

use crate::domain::{
    models::{AvatarIdentityRecord, AvatarImage, UserId},
    ports::outbound::AvatarRepository,
    AvatarError,
};

pub struct PostgresAvatarRepository {
    pool: PgPool,
}

impl PostgresAvatarRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AvatarRepository for PostgresAvatarRepository {
    async fn get_avatar(&self, user_id: &UserId) -> Result<Option<AvatarImage>, AvatarError> {
        let row = sqlx::query!(
            r#"
            SELECT image, mime_type
            FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| AvatarError::Storage(err.to_string()))?;

        Ok(row.map(|row| AvatarImage::new(row.image, row.mime_type)))
    }

    async fn set_avatar(&self, user_id: &UserId, image: &AvatarImage) -> Result<(), AvatarError> {
        sqlx::query!(
            r#"
            INSERT INTO user_avatars (user_id, image, mime_type, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (user_id) DO UPDATE
            SET image = EXCLUDED.image,
                mime_type = EXCLUDED.mime_type,
                updated_at = now()
            "#,
            user_id.as_i32(),
            &image.bytes,
            &image.mime_type,
        )
        .execute(&self.pool)
        .await
        .map_err(|err| AvatarError::Storage(err.to_string()))?;

        Ok(())
    }

    async fn delete_avatar(&self, user_id: &UserId) -> Result<(), AvatarError> {
        sqlx::query!(
            r#"
            DELETE FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id.as_i32(),
        )
        .execute(&self.pool)
        .await
        .map_err(|err| AvatarError::Storage(err.to_string()))?;

        Ok(())
    }

    async fn avatar_updated_at(
        &self,
        user_id: &UserId,
    ) -> Result<Option<time::OffsetDateTime>, AvatarError> {
        let updated_at = sqlx::query_scalar!(
            r#"
            SELECT updated_at
            FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| AvatarError::Storage(err.to_string()))?;

        Ok(updated_at)
    }

    async fn users_with_avatars_by_email(
        &self,
        emails: &[String],
    ) -> Result<Vec<AvatarIdentityRecord>, AvatarError> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }

        let normalized_emails = emails
            .iter()
            .map(|email| email.to_lowercase())
            .collect::<Vec<_>>();

        let rows = sqlx::query!(
            r#"
            SELECT
                users.id as "id!",
                lower(users.email) as "email!",
                user_avatars.updated_at as "updated_at!"
            FROM users
            INNER JOIN user_avatars ON user_avatars.user_id = users.id
            WHERE lower(users.email) = ANY($1)
            "#,
            &normalized_emails,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| AvatarError::Storage(err.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| AvatarIdentityRecord {
                user_id: UserId::new(row.id),
                email: row.email,
                updated_at: row.updated_at,
            })
            .collect())
    }
}
