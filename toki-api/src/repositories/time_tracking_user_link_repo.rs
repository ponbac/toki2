use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        NewTimeTrackingProviderUser, NewTimeTrackingUserLink, TimeTrackingProviderUser,
        TimeTrackingUserLink, UserId,
    },
    ports::outbound::TimeTrackingUserLinkRepository,
    TimeTrackingError,
};

use super::RepositoryError;

pub struct TimeTrackingUserLinkRepositoryImpl {
    pool: PgPool,
}

impl TimeTrackingUserLinkRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug)]
struct TimeTrackingUserLinkRow {
    id: i32,
    user_id: i32,
    provider: String,
    provider_company_id: String,
    provider_user_id: String,
    provider_user_email: Option<String>,
    provider_user_name: Option<String>,
    active: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    last_synced_at: OffsetDateTime,
}

impl From<TimeTrackingUserLinkRow> for TimeTrackingUserLink {
    fn from(row: TimeTrackingUserLinkRow) -> Self {
        Self {
            id: row.id,
            user_id: UserId::from(row.user_id),
            provider: row.provider,
            provider_company_id: row.provider_company_id,
            provider_user_id: row.provider_user_id,
            provider_user_email: row.provider_user_email,
            provider_user_name: row.provider_user_name,
            active: row.active,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_synced_at: row.last_synced_at,
        }
    }
}

fn map_repo_error(error: RepositoryError) -> TimeTrackingError {
    TimeTrackingError::unknown(error.to_string())
}

fn map_sqlx_error(error: sqlx::Error) -> TimeTrackingError {
    map_repo_error(RepositoryError::from(error))
}

#[async_trait]
impl TimeTrackingUserLinkRepository for TimeTrackingUserLinkRepositoryImpl {
    async fn upsert_provider_users(
        &self,
        users: &[NewTimeTrackingProviderUser],
    ) -> Result<Vec<TimeTrackingProviderUser>, TimeTrackingError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        let mut saved = Vec::with_capacity(users.len());

        for user in users {
            let row = sqlx::query_as!(
                TimeTrackingProviderUser,
                r#"
                INSERT INTO time_tracking_provider_users (
                    provider, provider_company_id, provider_user_id, foreign_id, internal_id,
                    name, email, active, last_synced_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP)
                ON CONFLICT (provider, provider_company_id, provider_user_id) DO UPDATE
                SET foreign_id = EXCLUDED.foreign_id,
                    internal_id = EXCLUDED.internal_id,
                    name = EXCLUDED.name,
                    email = EXCLUDED.email,
                    active = EXCLUDED.active,
                    last_synced_at = CURRENT_TIMESTAMP
                RETURNING id, provider, provider_company_id, provider_user_id, foreign_id,
                    internal_id, name, email, active, last_synced_at
                "#,
                user.provider,
                user.provider_company_id,
                user.provider_user_id,
                user.foreign_id,
                user.internal_id,
                user.name,
                user.email,
                user.active
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

            saved.push(row);
        }

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(saved)
    }

    async fn list_provider_users(
        &self,
        provider: &str,
        provider_company_id: &str,
    ) -> Result<Vec<TimeTrackingProviderUser>, TimeTrackingError> {
        sqlx::query_as!(
            TimeTrackingProviderUser,
            r#"
            SELECT id, provider, provider_company_id, provider_user_id, foreign_id,
                internal_id, name, email, active, last_synced_at
            FROM time_tracking_provider_users
            WHERE provider = $1 AND provider_company_id = $2
            ORDER BY active DESC, lower(name), provider_user_id
            "#,
            provider,
            provider_company_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn get_provider_user(
        &self,
        provider: &str,
        provider_company_id: &str,
        provider_user_id: &str,
    ) -> Result<Option<TimeTrackingProviderUser>, TimeTrackingError> {
        let row = sqlx::query_as!(
            TimeTrackingProviderUser,
            r#"
            SELECT id, provider, provider_company_id, provider_user_id, foreign_id,
                internal_id, name, email, active, last_synced_at
            FROM time_tracking_provider_users
            WHERE provider = $1 AND provider_company_id = $2 AND provider_user_id = $3
            "#,
            provider,
            provider_company_id,
            provider_user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row)
    }

    async fn list_active_links(
        &self,
        provider: &str,
        provider_company_id: &str,
    ) -> Result<Vec<TimeTrackingUserLink>, TimeTrackingError> {
        let rows = sqlx::query_as!(
            TimeTrackingUserLinkRow,
            r#"
            SELECT id, user_id, provider, provider_company_id, provider_user_id,
                provider_user_email, provider_user_name, active, created_at, updated_at,
                last_synced_at
            FROM time_tracking_user_links
            WHERE provider = $1 AND provider_company_id = $2 AND active
            ORDER BY updated_at DESC, id DESC
            "#,
            provider,
            provider_company_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_active_link_for_user(
        &self,
        user_id: &UserId,
        provider: &str,
    ) -> Result<Option<TimeTrackingUserLink>, TimeTrackingError> {
        let user_id = user_id.as_i32();
        let row = sqlx::query_as!(
            TimeTrackingUserLinkRow,
            r#"
            SELECT id, user_id, provider, provider_company_id, provider_user_id,
                provider_user_email, provider_user_name, active, created_at, updated_at,
                last_synced_at
            FROM time_tracking_user_links
            WHERE user_id = $1 AND provider = $2 AND active
            "#,
            user_id,
            provider
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(Into::into))
    }

    async fn upsert_active_link(
        &self,
        link: &NewTimeTrackingUserLink,
    ) -> Result<TimeTrackingUserLink, TimeTrackingError> {
        let user_id = link.user_id.as_i32();
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        sqlx::query!(
            r#"
            UPDATE time_tracking_user_links
            SET active = FALSE, updated_at = CURRENT_TIMESTAMP
            WHERE provider = $1
                AND provider_company_id = $2
                AND provider_user_id = $3
                AND user_id != $4
                AND active
            "#,
            link.provider,
            link.provider_company_id,
            link.provider_user_id,
            user_id
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let row = sqlx::query_as!(
            TimeTrackingUserLinkRow,
            r#"
            INSERT INTO time_tracking_user_links (
                user_id, provider, provider_company_id, provider_user_id,
                provider_user_email, provider_user_name, active, created_at, updated_at,
                last_synced_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, TRUE, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id, provider) WHERE active DO UPDATE
            SET provider_company_id = EXCLUDED.provider_company_id,
                provider_user_id = EXCLUDED.provider_user_id,
                provider_user_email = EXCLUDED.provider_user_email,
                provider_user_name = EXCLUDED.provider_user_name,
                updated_at = CURRENT_TIMESTAMP,
                last_synced_at = CURRENT_TIMESTAMP
            RETURNING id, user_id, provider, provider_company_id, provider_user_id,
                provider_user_email, provider_user_name, active, created_at, updated_at,
                last_synced_at
            "#,
            user_id,
            link.provider,
            link.provider_company_id,
            link.provider_user_id,
            link.provider_user_email,
            link.provider_user_name
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(row.into())
    }

    async fn deactivate_active_link(
        &self,
        user_id: &UserId,
        provider: &str,
    ) -> Result<(), TimeTrackingError> {
        let user_id = user_id.as_i32();
        sqlx::query!(
            r#"
            UPDATE time_tracking_user_links
            SET active = FALSE, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1 AND provider = $2 AND active
            "#,
            user_id,
            provider
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }
}
