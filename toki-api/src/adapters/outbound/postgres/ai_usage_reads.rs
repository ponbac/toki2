//! Reads that several AI usage repositories share, written once so they cannot
//! drift apart: the local date, the stored project mappings and personal keys,
//! and the project keys in a user's usage with their latest local date of use.

use time::Date;

use crate::{
    db::DbPool,
    domain::models::{
        AiMappedProject, AiStoredMapping, AiUsageTimeZone, ProjectId, TimeTrackingCompany, UserId,
    },
};

/// Today's date in `time_zone`, by the database's clock.
pub(super) async fn local_today(
    pool: &DbPool,
    time_zone: &AiUsageTimeZone,
) -> Result<Date, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (now() AT TIME ZONE $1)::date AS "today!""#,
        time_zone.as_str(),
    )
    .fetch_one(pool)
    .await
}

/// Every stored project mapping. Mappings are team-wide, one per key, so the
/// table stays small; reads resolve them with `AiMappingResolver`.
pub(super) async fn stored_mappings(pool: &DbPool) -> Result<Vec<AiStoredMapping>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT project_key, provider, provider_company_id, project_id, project_name
        FROM ai_project_mappings
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| AiStoredMapping {
            project_key: row.project_key,
            company: TimeTrackingCompany {
                provider: row.provider,
                company_id: row.provider_company_id,
            },
            project: AiMappedProject {
                id: ProjectId::new(row.project_id),
                name: row.project_name,
            },
        })
        .collect())
}

/// The project keys developers marked personal, of `user_id` or of everyone,
/// by user and key. `AiMappingResolver` decides which marks apply.
pub(super) async fn stored_personal_keys(
    pool: &DbPool,
    user_id: Option<&UserId>,
) -> Result<Vec<(UserId, String)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT user_id, project_key
        FROM ai_personal_project_keys
        WHERE $1::int4 IS NULL OR user_id = $1
        ORDER BY user_id, project_key
        "#,
        user_id.map(UserId::as_i32),
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| (UserId::new(row.user_id), row.project_key))
        .collect())
}

/// A project key in some usage.
pub(super) struct ProjectKeyUse {
    pub project_key: String,
    /// The latest local date with usage of the key.
    pub last_used_on: Date,
    /// Whether the key has a mapping, resolving or not.
    pub mapped: bool,
    /// Whether the user marked the key personal.
    pub personal: bool,
}

/// The project keys in the user's usage, most recently used first, each with
/// its latest local date of use in `time_zone`.
///
/// Skips from one of the user's keys to the next through the
/// (user_id, project_key, hour_start) index, then reads each key's latest hour
/// from the same index: the work grows with the number of keys, not with the
/// user's whole history.
pub(super) async fn project_key_usage(
    pool: &DbPool,
    user_id: &UserId,
    time_zone: &AiUsageTimeZone,
) -> Result<Vec<ProjectKeyUse>, sqlx::Error> {
    sqlx::query_as!(
        ProjectKeyUse,
        r#"
        WITH RECURSIVE keys (project_key) AS (
            SELECT min(project_key) FROM ai_usage_buckets WHERE user_id = $1
            UNION ALL
            SELECT (
                SELECT min(bucket.project_key)
                FROM ai_usage_buckets AS bucket
                WHERE bucket.user_id = $1 AND bucket.project_key > keys.project_key
            )
            FROM keys
            WHERE keys.project_key IS NOT NULL
        )
        SELECT
            keys.project_key AS "project_key!",
            ((
                SELECT max(bucket.hour_start)
                FROM ai_usage_buckets AS bucket
                WHERE bucket.user_id = $1 AND bucket.project_key = keys.project_key
            ) AT TIME ZONE $2)::date AS "last_used_on!",
            EXISTS (
                SELECT 1 FROM ai_project_mappings AS mapping
                WHERE mapping.project_key = keys.project_key
            ) AS "mapped!",
            EXISTS (
                SELECT 1 FROM ai_personal_project_keys AS personal
                WHERE personal.user_id = $1 AND personal.project_key = keys.project_key
            ) AS "personal!"
        FROM keys
        WHERE keys.project_key IS NOT NULL
        ORDER BY 2 DESC, 1
        "#,
        user_id.as_i32(),
        time_zone.as_str(),
    )
    .fetch_all(pool)
    .await
}
