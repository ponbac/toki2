use async_trait::async_trait;
use serde::Serialize;
use sqlx::PgPool;

use super::repo_error::RepositoryError;

#[async_trait]
pub trait TimerRepository {
    async fn get_timer_history(&self, user_id: &i32)
        -> Result<Vec<DatabaseTimer>, RepositoryError>;
    async fn active_timer(&self, user_id: &i32) -> Result<Option<DatabaseTimer>, RepositoryError>;
    async fn create_timer(&self, repository: &NewDatabaseTimer) -> Result<i32, RepositoryError>;
    async fn update_timer(&self, repository: &UpdateDatabaseTimer) -> Result<(), RepositoryError>;
    async fn save_active_timer(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
        registration_id: &str,
    ) -> Result<(), RepositoryError>;
    async fn delete_active_timer(&self, user_id: &i32) -> Result<(), RepositoryError>;
    async fn get_by_registration_id(
        &self,
        registration_id: &str,
    ) -> Result<Option<DatabaseTimer>, RepositoryError>;
    async fn update_start_and_end_time(
        &self,
        registration_id: &str,
        start_time: &time::OffsetDateTime,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    async fn update_times_and_registration_id(
        &self,
        old_registration_id: &str,
        new_registration_id: &str,
        start_time: &time::OffsetDateTime,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError>;
    async fn create_finished_timer(
        &self,
        timer: &FinishedDatabaseTimer,
    ) -> Result<i32, RepositoryError>;
}

pub struct TimerRepositoryImpl {
    pool: PgPool,
}

impl TimerRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTimer {
    pub id: i32,
    pub registration_id: Option<String>,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<time::OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

pub struct NewDatabaseTimer {
    pub user_id: i32,
    pub start_time: time::OffsetDateTime,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: String,
}

pub struct UpdateDatabaseTimer {
    pub user_id: i32,
    pub user_note: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub start_time: Option<time::OffsetDateTime>,
}

pub struct FinishedDatabaseTimer {
    pub user_id: i32,
    pub start_time: time::OffsetDateTime,
    pub end_time: time::OffsetDateTime,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: String,
    pub registration_id: String,
}

#[async_trait]
impl TimerRepository for TimerRepositoryImpl {
    async fn get_timer_history(
        &self,
        user_id: &i32,
    ) -> Result<Vec<DatabaseTimer>, RepositoryError> {
        let timers = sqlx::query_as!(
            DatabaseTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at, registration_id
            FROM timer_history
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(timers)
    }

    async fn active_timer(&self, user_id: &i32) -> Result<Option<DatabaseTimer>, RepositoryError> {
        let single_timer = sqlx::query_as!(
            DatabaseTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at, registration_id
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

    async fn create_timer(&self, timer: &NewDatabaseTimer) -> Result<i32, RepositoryError> {
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

    async fn update_timer(&self, timer: &UpdateDatabaseTimer) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE timer_history
            SET note = $1, project_id = $2, project_name = $3, activity_id = $4, activity_name = $5, start_time = COALESCE($7, start_time)
            WHERE user_id = $6 AND end_time IS NULL
            "#,
            timer.user_note,
            timer.project_id,
            timer.project_name,
            timer.activity_id,
            timer.activity_name,
            timer.user_id,
            timer.start_time
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

    async fn get_by_registration_id(
        &self,
        registration_id: &str,
    ) -> Result<Option<DatabaseTimer>, RepositoryError> {
        let timer = sqlx::query_as!(
            DatabaseTimer,
            r#"
            SELECT * FROM timer_history WHERE registration_id = $1
            "#,
            registration_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(timer)
    }

    async fn update_start_and_end_time(
        &self,
        registration_id: &str,
        start_time: &time::OffsetDateTime,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let query_result = sqlx::query!(
            r#"
            UPDATE timer_history SET start_time = $1, end_time = $2 WHERE registration_id = $3
            "#,
            start_time,
            end_time,
            registration_id
        )
        .execute(&self.pool)
        .await?;

        if query_result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(registration_id.to_string()));
        }

        Ok(())
    }

    async fn update_times_and_registration_id(
        &self,
        old_registration_id: &str,
        new_registration_id: &str,
        start_time: &time::OffsetDateTime,
        end_time: &time::OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        // Single statement: set times and new registration_id where old registration_id matches
        let query_result = sqlx::query!(
            r#"
            UPDATE timer_history
            SET start_time = $1, end_time = $2, registration_id = $4
            WHERE registration_id = $3
            "#,
            start_time,
            end_time,
            old_registration_id,
            new_registration_id
        )
        .execute(&self.pool)
        .await?;

        if query_result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(old_registration_id.to_string()));
        }

        Ok(())
    }

    async fn create_finished_timer(
        &self,
        timer: &FinishedDatabaseTimer,
    ) -> Result<i32, RepositoryError> {
        let id = sqlx::query!(
            r#"
            INSERT INTO timer_history (
                user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, registration_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id
            "#,
            timer.user_id,
            timer.start_time,
            timer.end_time,
            timer.project_id,
            timer.project_name,
            timer.activity_id,
            timer.activity_name,
            timer.note,
            timer.registration_id
        )
        .fetch_one(&self.pool)
        .await?
        .id;

        Ok(id)
    }
}
