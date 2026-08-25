use async_trait::async_trait;
use sqlx::{postgres::PgRow, Row};

use super::{FinishedDatabaseTimer, RepositoryError, TimerRepositoryImpl};

#[derive(Debug)]
pub enum DatabaseBeginTimeEntryWrite {
    Fresh {
        operation_id: String,
        prepared: serde_json::Value,
    },
    Pending {
        operation_id: String,
        prepared: serde_json::Value,
    },
    Replay {
        prepared: serde_json::Value,
        registration_id: String,
    },
    NoActiveTimer,
    PayloadMismatch,
}

#[async_trait]
pub trait TimeEntryWriteRepository {
    async fn begin_save_active_timer(
        &self,
        user_id: i32,
        key: &str,
        fingerprint: &str,
        operation_id: &str,
        stopped_at: time::OffsetDateTime,
        note_override: Option<&str>,
    ) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError>;

    async fn begin_create_time_entry(
        &self,
        user_id: i32,
        key: &str,
        fingerprint: &str,
        operation_id: &str,
        prepared: serde_json::Value,
    ) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError>;

    async fn complete_time_entry_write(
        &self,
        operation: &str,
        key: &str,
        active_timer_id: Option<i32>,
        timer: &FinishedDatabaseTimer,
    ) -> Result<(), RepositoryError>;

    async fn cancel_time_entry_write(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
    ) -> Result<(), RepositoryError>;
}

impl TimerRepositoryImpl {
    async fn classify_existing_write(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
        fingerprint: &str,
    ) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError> {
        let existing = sqlx::query(
            r#"
            SELECT request_fingerprint, provider_operation_id, state,
                   prepared_write, registration_id
            FROM time_tracking_idempotency
            WHERE user_id = $1 AND operation = $2 AND idempotency_key = $3
            "#,
        )
        .bind(user_id)
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            return Ok(DatabaseBeginTimeEntryWrite::NoActiveTimer);
        };
        if existing.try_get::<String, _>("request_fingerprint")? != fingerprint {
            return Ok(DatabaseBeginTimeEntryWrite::PayloadMismatch);
        }

        let prepared = existing.try_get("prepared_write")?;
        if existing.try_get::<String, _>("state")? == "completed" {
            return Ok(DatabaseBeginTimeEntryWrite::Replay {
                prepared,
                registration_id: existing.try_get("registration_id")?,
            });
        }

        Ok(DatabaseBeginTimeEntryWrite::Pending {
            operation_id: existing.try_get("provider_operation_id")?,
            prepared,
        })
    }

    fn fresh_write(row: PgRow) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError> {
        Ok(DatabaseBeginTimeEntryWrite::Fresh {
            operation_id: row.try_get("provider_operation_id")?,
            prepared: row.try_get("prepared_write")?,
        })
    }
}

#[async_trait]
impl TimeEntryWriteRepository for TimerRepositoryImpl {
    async fn begin_save_active_timer(
        &self,
        user_id: i32,
        key: &str,
        fingerprint: &str,
        operation_id: &str,
        stopped_at: time::OffsetDateTime,
        note_override: Option<&str>,
    ) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError> {
        let inserted = sqlx::query(
            r#"
            INSERT INTO time_tracking_idempotency (
                user_id, operation, idempotency_key, request_fingerprint,
                provider_operation_id, prepared_write
            )
            SELECT $1, 'save_active_timer', $2, $3, $4,
                   jsonb_build_object(
                       'version', 1,
                       'origin', jsonb_build_object(
                           'kind', 'activeTimer',
                           'timerHistoryId', timer.id
                       ),
                       'projectId', COALESCE(timer.project_id, ''),
                       'projectName', COALESCE(timer.project_name, ''),
                       'activityId', COALESCE(timer.activity_id, ''),
                       'activityName', COALESCE(timer.activity_name, ''),
                       'startTime', timer.start_time,
                       'endTime', $5::timestamptz,
                       'note', COALESCE($6, timer.note, '')
                   )
            FROM timer_history AS timer
            WHERE timer.user_id = $1 AND timer.end_time IS NULL
            ON CONFLICT (user_id, operation, idempotency_key) DO NOTHING
            RETURNING provider_operation_id, prepared_write
            "#,
        )
        .bind(user_id)
        .bind(key)
        .bind(fingerprint)
        .bind(operation_id)
        .bind(stopped_at)
        .bind(note_override)
        .fetch_optional(&self.pool)
        .await?;

        match inserted {
            Some(row) => Self::fresh_write(row),
            None => {
                self.classify_existing_write(user_id, "save_active_timer", key, fingerprint)
                    .await
            }
        }
    }

    async fn begin_create_time_entry(
        &self,
        user_id: i32,
        key: &str,
        fingerprint: &str,
        operation_id: &str,
        prepared: serde_json::Value,
    ) -> Result<DatabaseBeginTimeEntryWrite, RepositoryError> {
        let inserted = sqlx::query(
            r#"
            INSERT INTO time_tracking_idempotency (
                user_id, operation, idempotency_key, request_fingerprint,
                provider_operation_id, prepared_write
            )
            VALUES ($1, 'create_time_entry', $2, $3, $4, $5)
            ON CONFLICT (user_id, operation, idempotency_key) DO NOTHING
            RETURNING provider_operation_id, prepared_write
            "#,
        )
        .bind(user_id)
        .bind(key)
        .bind(fingerprint)
        .bind(operation_id)
        .bind(prepared)
        .fetch_optional(&self.pool)
        .await?;

        match inserted {
            Some(row) => Self::fresh_write(row),
            None => {
                self.classify_existing_write(user_id, "create_time_entry", key, fingerprint)
                    .await
            }
        }
    }

    async fn complete_time_entry_write(
        &self,
        operation: &str,
        key: &str,
        active_timer_id: Option<i32>,
        timer: &FinishedDatabaseTimer,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await?;
        let history_update = match active_timer_id {
            Some(timer_id) => {
                let mut executor = transaction.executor();
                sqlx::query(
                    r#"
                    UPDATE timer_history
                    SET start_time = $1, end_time = $2,
                        project_id = $3, project_name = $4,
                        activity_id = $5, activity_name = $6,
                        note = $7, registration_id = $8
                    WHERE id = $9 AND user_id = $10 AND end_time IS NULL
                    "#,
                )
                .bind(timer.start_time)
                .bind(timer.end_time)
                .bind(&timer.project_id)
                .bind(&timer.project_name)
                .bind(&timer.activity_id)
                .bind(&timer.activity_name)
                .bind(&timer.note)
                .bind(&timer.registration_id)
                .bind(timer_id)
                .bind(timer.user_id)
                .execute(&mut executor)
                .await?
            }
            None => {
                let mut executor = transaction.executor();
                sqlx::query(
                    r#"
                    INSERT INTO timer_history (
                        user_id, start_time, end_time, project_id, project_name,
                        activity_id, activity_name, note, registration_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
                .execute(&mut executor)
                .await?
            }
        };
        if history_update.rows_affected() != 1 {
            return Err(RepositoryError::NotFound(format!(
                "prepared timer history for idempotency key {key}"
            )));
        }

        let idempotency_update = {
            let mut executor = transaction.executor();
            sqlx::query(
                r#"
                UPDATE time_tracking_idempotency
                SET state = 'completed', registration_id = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE user_id = $1 AND operation = $2
                  AND idempotency_key = $3 AND state = 'pending'
                "#,
            )
            .bind(timer.user_id)
            .bind(operation)
            .bind(key)
            .bind(&timer.registration_id)
            .execute(&mut executor)
            .await?
        };
        if idempotency_update.rows_affected() != 1 {
            return Err(RepositoryError::NotFound(format!(
                "pending idempotency record {key}"
            )));
        }

        transaction.commit().await?;
        Ok(())
    }

    async fn cancel_time_entry_write(
        &self,
        user_id: i32,
        operation: &str,
        key: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM time_tracking_idempotency
            WHERE user_id = $1 AND operation = $2
              AND idempotency_key = $3 AND state = 'pending'
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
    use crate::{
        config::DatabaseSettings,
        repositories::{NewDatabaseTimer, TimerRepository},
    };

    #[tokio::test]
    #[ignore = "requires an isolated TOKI_TEST_DATABASE_URL"]
    async fn prepared_writes_are_atomic_and_replayable_in_postgres() {
        let database_url = std::env::var("TOKI_TEST_DATABASE_URL")
            .expect("TOKI_TEST_DATABASE_URL must point to an isolated migrated database");
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
        let user_id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO users (email, full_name, picture, access_token)
            VALUES ($1, 'Write Safety Test', '', '')
            RETURNING id
            "#,
        )
        .bind(format!(
            "write-safety-{}@example.com",
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ))
        .fetch_one(&pool)
        .await
        .expect("create test user");

        let started_at = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
        let active_timer = NewDatabaseTimer {
            user_id,
            start_time: started_at,
            project_id: Some("project-1".to_string()),
            project_name: Some("Project".to_string()),
            activity_id: Some("activity-1".to_string()),
            activity_name: Some("Activity".to_string()),
            note: "work".to_string(),
        };
        repo.create_timer(&active_timer)
            .await
            .expect("create active timer");
        let duplicate = repo
            .create_timer(&active_timer)
            .await
            .expect_err("active timers are unique per user");
        assert!(matches!(
            duplicate,
            RepositoryError::DatabaseError(sqlx::Error::Database(ref database))
                if database.constraint() == Some("idx_timer_history_one_active_per_user")
        ));

        let stopped_at = time::OffsetDateTime::now_utc();
        let (first, second) = tokio::join!(
            repo.begin_save_active_timer(
                user_id,
                "save-key",
                "save-fingerprint-v1",
                "provider-operation-1",
                stopped_at,
                None,
            ),
            repo.begin_save_active_timer(
                user_id,
                "save-key",
                "save-fingerprint-v1",
                "provider-operation-1",
                stopped_at + time::Duration::minutes(1),
                None,
            ),
        );
        let (fresh, pending) = match (first.unwrap(), second.unwrap()) {
            (
                DatabaseBeginTimeEntryWrite::Fresh {
                    prepared: fresh, ..
                },
                DatabaseBeginTimeEntryWrite::Pending {
                    prepared: pending, ..
                },
            )
            | (
                DatabaseBeginTimeEntryWrite::Pending {
                    prepared: pending, ..
                },
                DatabaseBeginTimeEntryWrite::Fresh {
                    prepared: fresh, ..
                },
            ) => (fresh, pending),
            _ => panic!("exactly one concurrent save must be fresh"),
        };
        assert_eq!(pending, fresh);
        let prepared = fresh;
        assert_eq!(prepared["version"], 1);
        let persisted_end = time::OffsetDateTime::parse(
            prepared["endTime"].as_str().expect("stored RFC3339 time"),
            &time::format_description::well_known::Rfc3339,
        )
        .expect("parse stored time");
        assert!((stopped_at - persisted_end).abs() < time::Duration::microseconds(1));
        assert!(matches!(
            repo.begin_save_active_timer(
                user_id,
                "save-key",
                "different-fingerprint",
                "provider-operation-1",
                stopped_at,
                None,
            )
            .await
            .unwrap(),
            DatabaseBeginTimeEntryWrite::PayloadMismatch
        ));

        let finished = FinishedDatabaseTimer {
            user_id,
            start_time: started_at,
            end_time: persisted_end,
            project_id: Some("project-1".to_string()),
            project_name: Some("Project".to_string()),
            activity_id: Some("activity-1".to_string()),
            activity_name: Some("Activity".to_string()),
            note: "work".to_string(),
            registration_id: "entry-1".to_string(),
        };
        repo.complete_time_entry_write(
            "save_active_timer",
            "save-key",
            Some(prepared["origin"]["timerHistoryId"].as_i64().unwrap() as i32),
            &finished,
        )
        .await
        .expect("complete save");
        assert!(matches!(
            repo.begin_save_active_timer(
                user_id,
                "save-key",
                "save-fingerprint-v1",
                "provider-operation-1",
                stopped_at,
                None,
            )
            .await
            .unwrap(),
            DatabaseBeginTimeEntryWrite::Replay { registration_id, .. }
                if registration_id == "entry-1"
        ));

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("clean up test user");
    }
}
