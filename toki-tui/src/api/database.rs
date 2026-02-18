use anyhow::Result;
use sqlx::PgPool;
use time::OffsetDateTime;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimerHistoryEntry {
    pub id: i32,
    pub user_id: i32,
    pub start_time: OffsetDateTime,
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }

    /// Save a completed timer entry to history
    pub async fn save_timer_entry(
        &self,
        user_id: i32,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<TimerHistoryEntry> {
        let entry = sqlx::query_as::<_, TimerHistoryEntry>(
            r#"
            INSERT INTO timer_history
                (user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note
            "#,
        )
        .bind(user_id)
        .bind(start_time)
        .bind(end_time)
        .bind(project_id)
        .bind(project_name)
        .bind(activity_id)
        .bind(activity_name)
        .bind(note)
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }

    /// Get timer history for a user (most recent first)
    pub async fn get_timer_history(&self, user_id: i32, limit: i64) -> Result<Vec<TimerHistoryEntry>> {
        let timers = sqlx::query_as::<_, TimerHistoryEntry>(
            r#"
            SELECT id, user_id, start_time, end_time, project_id, project_name,
                   activity_id, activity_name, note
            FROM timer_history
            WHERE user_id = $1
            ORDER BY start_time DESC
            LIMIT $2
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(timers)
    }

    /// Update an existing timer entry
    pub async fn update_timer_entry(
        &self,
        entry_id: i32,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<TimerHistoryEntry> {
        let entry = sqlx::query_as::<_, TimerHistoryEntry>(
            r#"
            UPDATE timer_history
            SET start_time = $2,
                end_time = $3,
                project_id = $4,
                project_name = $5,
                activity_id = $6,
                activity_name = $7,
                note = $8
            WHERE id = $1
            RETURNING id, user_id, start_time, end_time, project_id, project_name, activity_id, activity_name, note
            "#,
        )
        .bind(entry_id)
        .bind(start_time)
        .bind(end_time)
        .bind(project_id)
        .bind(project_name)
        .bind(activity_id)
        .bind(activity_name)
        .bind(note)
        .fetch_one(&self.pool)
        .await?;

        Ok(entry)
    }
}
