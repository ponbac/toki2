use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use strum::{Display, EnumString};

use super::repo_error::RepositoryError;

pub trait MilltimeRepository {
    async fn get_timer_history(&self, user_id: &i32)
        -> Result<Vec<MilltimeTimer>, RepositoryError>;
    async fn active_timer(&self, user_id: &i32) -> Result<Option<MilltimeTimer>, RepositoryError>;
    async fn create_timer(&self, repository: &NewMilltimeTimer) -> Result<i32, RepositoryError>;
    async fn update_timer(&self, repository: &UpdateMilltimeTimer) -> Result<(), RepositoryError>;
    async fn save_active_timer(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
        registration_id: &str,
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

#[derive(Debug, Serialize, Deserialize, EnumString, Display, PartialEq, Eq)]
pub enum TimerType {
    #[strum(ascii_case_insensitive, serialize = "milltime")]
    Milltime,
    #[strum(ascii_case_insensitive, serialize = "standalone")]
    Standalone,
}

impl From<String> for TimerType {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "milltime" => TimerType::Milltime,
            "standalone" => TimerType::Standalone,
            _ => panic!("Invalid timer type"),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MilltimeTimer {
    pub timer_type: TimerType,
    pub id: i32,
    pub registration_id: Option<String>,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<time::OffsetDateTime>,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    pub note: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

pub struct NewMilltimeTimer {
    pub user_id: i32,
    pub timer_type: TimerType,
    pub start_time: time::OffsetDateTime,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    pub note: String,
}

pub struct UpdateMilltimeTimer {
    pub user_id: i32,
    pub user_note: String,
}

impl MilltimeRepository for MilltimeRepositoryImpl {
    async fn get_timer_history(
        &self,
        user_id: &i32,
    ) -> Result<Vec<MilltimeTimer>, RepositoryError> {
        let timers = sqlx::query_as!(
            MilltimeTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at, registration_id, timer_type
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
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at, registration_id, timer_type
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
            INSERT INTO timer_history (user_id, start_time, project_id, project_name, activity_id, activity_name, note, timer_type)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
            timer.user_id,
            timer.start_time,
            timer.project_id,
            timer.project_name,
            timer.activity_id,
            timer.activity_name,
            timer.note,
            timer.timer_type.to_string()
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
        registration_id: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE timer_history
            SET end_time = $1, registration_id = $2
            WHERE user_id = $3 AND end_time IS NULL
            "#,
            end_time,
            registration_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_timer(&self, timer: &UpdateMilltimeTimer) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE timer_history
            SET note = $1
            WHERE user_id = $2 AND end_time IS NULL
            "#,
            timer.user_note,
            timer.user_id
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
