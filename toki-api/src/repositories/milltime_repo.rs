use serde::Serialize;
use sqlx::PgPool;

use super::repo_error::RepositoryError;

pub trait MilltimeRepository {
    async fn get_timer_history(&self, user_id: &i32)
        -> Result<Vec<MilltimeTimer>, RepositoryError>;
    async fn active_timer(&self, user_id: &i32) -> Result<Option<MilltimeTimer>, RepositoryError>;
    async fn create_timer(&self, repository: &NewMilltimeTimer) -> Result<i32, RepositoryError>;
    async fn save_active_timer(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    async fn delete_active_timer(&self, user_id: &i32) -> Result<(), RepositoryError>;
}

pub struct MilltimeRepositoryImpl {
    pool: PgPool,
}

impl MilltimeRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MilltimeTimer {
    pub id: i32,
    pub user_id: i32,
    pub start_time: time::OffsetDateTime,
    pub end_time: Option<time::OffsetDateTime>,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    pub note: String,
    pub created_at: time::OffsetDateTime,
}

pub struct NewMilltimeTimer {
    pub user_id: i32,
    pub start_time: time::OffsetDateTime,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    pub note: String,
}

impl MilltimeRepository for MilltimeRepositoryImpl {
    async fn get_timer_history(
        &self,
        user_id: &i32,
    ) -> Result<Vec<MilltimeTimer>, RepositoryError> {
        let timers = sqlx::query_as!(
            MilltimeTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at
            FROM timer_history
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(timers)
    }

    async fn active_timer(&self, user_id: &i32) -> Result<Option<MilltimeTimer>, RepositoryError> {
        let single_timer = sqlx::query_as!(
            MilltimeTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at
            FROM timer_history
            WHERE user_id = $1 AND end_time IS NULL
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await;

        match single_timer {
            Ok(timer) => Ok(timer),
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(e) => {
                // multiple rows found, delete all but the most recent
                let most_recent_timer_id = sqlx::query!(
                    r#"
                    SELECT id
                    FROM timer_history
                    WHERE user_id = $1 AND end_time IS NULL
                    ORDER BY created_at DESC
                    LIMIT 1
                    "#,
                    user_id
                )
                .fetch_one(&self.pool)
                .await?
                .id;

                sqlx::query!(
                    r#"
                    DELETE FROM timer_history
                    WHERE user_id = $1 AND end_time IS NULL AND id != $2
                    "#,
                    user_id,
                    most_recent_timer_id
                )
                .execute(&self.pool)
                .await?;

                Err(e.into())
            }
        }
    }

    async fn create_timer(&self, timer: &NewMilltimeTimer) -> Result<i32, RepositoryError> {
        let id = sqlx::query!(
            r#"
            INSERT INTO timer_history (user_id, start_time, project_id, project_name, activity_id, activity_name, note)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
            timer.user_id,
            timer.start_time,
            timer.project_id,
            timer.project_name,
            timer.activity_id,
            timer.activity_name,
            timer.note
        )
        .fetch_one(&self.pool)
        .await?
        .id;

        Ok(id)
    }

    async fn save_active_timer(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE timer_history
            SET end_time = $1
            WHERE user_id = $2 AND end_time IS NULL
            "#,
            end_time,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_active_timer(&self, user_id: &i32) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            DELETE FROM timer_history
            WHERE user_id = $1 AND end_time IS NULL
            "#,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
