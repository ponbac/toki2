use async_trait::async_trait;
use serde::Serialize;
use sqlx::Row;

use crate::db::DbPool;

use super::repo_error::RepositoryError;

#[async_trait]
pub trait TimerRepository {
    async fn get_timer_history(&self, user_id: &i32)
        -> Result<Vec<DatabaseTimer>, RepositoryError>;
    async fn active_timer(&self, user_id: &i32) -> Result<Option<DatabaseTimer>, RepositoryError>;
    async fn create_timer(&self, repository: &NewDatabaseTimer) -> Result<i32, RepositoryError>;
    async fn update_timer(&self, repository: &UpdateDatabaseTimer) -> Result<(), RepositoryError>;
    async fn finish_active_timer_idempotently(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
        registration_id: &str,
        key: &str,
        result: serde_json::Value,
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
    async fn create_finished_timer_idempotently(
        &self,
        timer: &FinishedDatabaseTimer,
        key: &str,
        result: serde_json::Value,
    ) -> Result<i32, RepositoryError>;
    async fn claim_idempotency(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
        request_hash: &str,
        operation_id: &str,
    ) -> Result<DatabaseIdempotencyClaim, RepositoryError>;
    async fn release_idempotency(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
    ) -> Result<(), RepositoryError>;
}

#[derive(Debug)]
pub enum DatabaseIdempotencyClaim {
    Fresh { operation_id: String },
    Resumed { operation_id: String },
    Replay(serde_json::Value),
    InProgress,
    PayloadMismatch,
}

pub struct TimerRepositoryImpl {
    pool: DbPool,
}

impl TimerRepositoryImpl {
    pub fn new(pool: DbPool) -> Self {
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
        let timer = sqlx::query_as!(
            DatabaseTimer,
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note, created_at, registration_id
            FROM timer_history
            WHERE user_id = $1 AND end_time IS NULL
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(timer)
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

    async fn finish_active_timer_idempotently(
        &self,
        user_id: &i32,
        end_time: &time::OffsetDateTime,
        registration_id: &str,
        key: &str,
        result: serde_json::Value,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await?;
        let timer_update = {
            let mut executor = transaction.executor();
            sqlx::query(
                r#"
                UPDATE timer_history
                SET end_time = $1, registration_id = $2
                WHERE user_id = $3 AND end_time IS NULL
                "#,
            )
            .bind(end_time)
            .bind(registration_id)
            .bind(user_id)
            .execute(&mut executor)
            .await?
        };

        if timer_update.rows_affected() != 1 {
            return Err(RepositoryError::NotFound(format!(
                "active timer for user {user_id}"
            )));
        }

        let idempotency_update = {
            let mut executor = transaction.executor();
            sqlx::query(
                r#"
                UPDATE time_tracking_idempotency
                SET state = 'completed', result = $3, updated_at = CURRENT_TIMESTAMP
                WHERE user_id = $1
                  AND operation = 'save_active_timer'
                  AND idempotency_key = $2
                  AND state = 'pending'
                "#,
            )
            .bind(user_id)
            .bind(key)
            .bind(result)
            .execute(&mut executor)
            .await?
        };

        if idempotency_update.rows_affected() != 1 {
            return Err(RepositoryError::NotFound(format!(
                "save idempotency record {key}"
            )));
        }

        transaction.commit().await?;

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

    async fn create_finished_timer_idempotently(
        &self,
        timer: &FinishedDatabaseTimer,
        key: &str,
        result: serde_json::Value,
    ) -> Result<i32, RepositoryError> {
        let mut transaction = self.pool.begin().await?;
        let id: i32 = {
            let mut executor = transaction.executor();
            sqlx::query_scalar(
                r#"
                INSERT INTO timer_history (
                    user_id, start_time, end_time, project_id, project_name,
                    activity_id, activity_name, note, registration_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING id
                "#,
            )
            .bind(timer.user_id)
            .bind(timer.start_time)
            .bind(timer.end_time)
            .bind(&timer.project_id)
            .bind(&timer.project_name)
            .bind(&timer.activity_id)
            .bind(&timer.activity_name)
            .bind(&timer.note)
            .bind(&timer.registration_id)
            .fetch_one(&mut executor)
            .await?
        };

        let idempotency_update = {
            let mut executor = transaction.executor();
            sqlx::query(
                r#"
                UPDATE time_tracking_idempotency
                SET state = 'completed', result = $3, updated_at = CURRENT_TIMESTAMP
                WHERE user_id = $1
                  AND operation = 'create_time_entry'
                  AND idempotency_key = $2
                  AND state = 'pending'
                "#,
            )
            .bind(timer.user_id)
            .bind(key)
            .bind(result)
            .execute(&mut executor)
            .await?
        };

        if idempotency_update.rows_affected() != 1 {
            return Err(RepositoryError::NotFound(format!(
                "create idempotency record {key}"
            )));
        }

        transaction.commit().await?;

        Ok(id)
    }

    async fn claim_idempotency(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
        request_hash: &str,
        operation_id: &str,
    ) -> Result<DatabaseIdempotencyClaim, RepositoryError> {
        let inserted = sqlx::query(
            r#"
            INSERT INTO time_tracking_idempotency (
                user_id, operation, idempotency_key, request_hash,
                provider_operation_id, locked_until
            )
            VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP + INTERVAL '60 seconds')
            ON CONFLICT (user_id, operation, idempotency_key) DO NOTHING
            RETURNING provider_operation_id
            "#,
        )
        .bind(user_id)
        .bind(operation)
        .bind(key)
        .bind(request_hash)
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = inserted {
            return Ok(DatabaseIdempotencyClaim::Fresh {
                operation_id: row.try_get("provider_operation_id")?,
            });
        }

        let existing = sqlx::query(
            r#"
            SELECT request_hash, provider_operation_id, state, result
            FROM time_tracking_idempotency
            WHERE user_id = $1 AND operation = $2 AND idempotency_key = $3
            "#,
        )
        .bind(user_id)
        .bind(operation)
        .bind(key)
        .fetch_one(&self.pool)
        .await?;

        let stored_hash: String = existing.try_get("request_hash")?;
        if stored_hash != request_hash {
            return Ok(DatabaseIdempotencyClaim::PayloadMismatch);
        }

        let state: String = existing.try_get("state")?;
        if state == "completed" {
            return Ok(DatabaseIdempotencyClaim::Replay(
                existing.try_get("result")?,
            ));
        }

        let resumed = sqlx::query(
            r#"
            UPDATE time_tracking_idempotency
            SET locked_until = CURRENT_TIMESTAMP + INTERVAL '60 seconds',
                updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1
              AND operation = $2
              AND idempotency_key = $3
              AND state = 'pending'
              AND locked_until <= CURRENT_TIMESTAMP
            RETURNING provider_operation_id
            "#,
        )
        .bind(user_id)
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(match resumed {
            Some(row) => DatabaseIdempotencyClaim::Resumed {
                operation_id: row.try_get("provider_operation_id")?,
            },
            None => DatabaseIdempotencyClaim::InProgress,
        })
    }

    async fn release_idempotency(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE time_tracking_idempotency
            SET locked_until = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
            WHERE user_id = $1 AND operation = $2 AND idempotency_key = $3 AND state = 'pending'
            "#,
        )
        .bind(user_id)
        .bind(operation)
        .bind(key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod persistence_tests {
    use sqlx::postgres::PgPoolOptions;

    use super::*;
    use crate::config::DatabaseSettings;

    #[tokio::test]
    async fn active_timer_and_idempotency_constraints_hold_in_postgres() {
        let Ok(database_url) = std::env::var("TOKI_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&database_url)
            .await
            .expect("connect to test database");
        let settings = DatabaseSettings {
            username: "test".to_string(),
            password: "test".to_string(),
            port: 0,
            host: "test".to_string(),
            database_name: "toki-test".to_string(),
            require_ssl: false,
        };
        let repo = TimerRepositoryImpl::new(crate::db::traced_pool(pool.clone(), &settings));
        let email = format!(
            "write-safety-{}@example.com",
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        );
        let user_id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO users (email, full_name, picture, access_token)
            VALUES ($1, 'Write Safety Test', '', '')
            RETURNING id
            "#,
        )
        .bind(email)
        .fetch_one(&pool)
        .await
        .expect("create test user");

        let timer = NewDatabaseTimer {
            user_id,
            start_time: time::OffsetDateTime::now_utc() - time::Duration::hours(1),
            project_id: Some("project-1".to_string()),
            project_name: Some("Project".to_string()),
            activity_id: Some("activity-1".to_string()),
            activity_name: Some("Activity".to_string()),
            note: "work".to_string(),
        };
        repo.create_timer(&timer).await.expect("first active timer");
        let duplicate = repo
            .create_timer(&timer)
            .await
            .expect_err("second active timer must conflict");
        assert!(matches!(
            duplicate,
            RepositoryError::DatabaseError(sqlx::Error::Database(ref database))
                if database.constraint() == Some("idx_timer_history_one_active_per_user")
        ));

        let key = "postgres-save-key";
        let operation_id = "toki-op-postgres-test";
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "save_active_timer",
                key,
                "request-hash",
                operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::Fresh { .. }
        ));
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "save_active_timer",
                key,
                "request-hash",
                operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::InProgress
        ));
        repo.release_idempotency(user_id, "save_active_timer", key)
            .await
            .unwrap();
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "save_active_timer",
                key,
                "request-hash",
                operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::Resumed { .. }
        ));

        let end_time = time::OffsetDateTime::now_utc();
        let result = serde_json::json!({ "registrationId": "entry-1" });
        repo.finish_active_timer_idempotently(&user_id, &end_time, "entry-1", key, result.clone())
            .await
            .expect("finish timer and idempotency atomically");
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "save_active_timer",
                key,
                "request-hash",
                operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::Replay(replayed) if replayed == result
        ));
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "save_active_timer",
                key,
                "different-hash",
                operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::PayloadMismatch
        ));

        let state: String = sqlx::query_scalar(
            r#"
            SELECT state FROM time_tracking_idempotency
            WHERE user_id = $1 AND operation = 'save_active_timer' AND idempotency_key = $2
            "#,
        )
        .bind(user_id)
        .bind(key)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, "completed");

        let create_key = "postgres-create-key";
        let create_operation_id = "toki-op-postgres-create-test";
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "create_time_entry",
                create_key,
                "create-request-hash",
                create_operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::Fresh { .. }
        ));

        let finished_timer = FinishedDatabaseTimer {
            user_id,
            start_time: end_time,
            end_time: end_time + time::Duration::hours(1),
            project_id: Some("project-1".to_string()),
            project_name: Some("Project".to_string()),
            activity_id: Some("activity-1".to_string()),
            activity_name: Some("Activity".to_string()),
            note: "created directly".to_string(),
            registration_id: "entry-2".to_string(),
        };
        let create_result = serde_json::json!({ "registrationId": "entry-2" });
        repo.create_finished_timer_idempotently(&finished_timer, create_key, create_result.clone())
            .await
            .expect("create timer and complete idempotency atomically");
        assert!(matches!(
            repo.claim_idempotency(
                user_id,
                "create_time_entry",
                create_key,
                "create-request-hash",
                create_operation_id,
            )
            .await
            .unwrap(),
            DatabaseIdempotencyClaim::Replay(replayed) if replayed == create_result
        ));

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("clean up test user");
    }
}
