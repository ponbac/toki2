//! Reads that several AI usage repositories share, written once so they cannot
//! drift apart: the local date, the stored project mappings, and the project
//! keys in some usage with their latest local date of use.

use time::Date;

use crate::{
    db::DbPool,
    domain::models::{
        AiMappedProject, AiStoredMapping, AiUsageScope, AiUsageTimeZone, ProjectId,
        TimeTrackingCompany,
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

/// A project key in some usage.
pub(super) struct ProjectKeyUse {
    pub project_key: String,
    /// The latest local date with usage of the key.
    pub last_used_on: Date,
    /// Whether the key has a mapping, resolving or not.
    pub mapped: bool,
}

/// The project keys in the scope's usage, most recently used first, each with
/// its latest local date of use in `time_zone`.
pub(super) async fn project_key_usage(
    pool: &DbPool,
    scope: AiUsageScope,
    time_zone: &AiUsageTimeZone,
) -> Result<Vec<ProjectKeyUse>, sqlx::Error> {
    match scope {
        // Skips from one of the user's keys to the next through the
        // (user_id, project_key, hour_start) index, then reads each key's
        // latest hour from the same index: the work grows with the number of
        // keys, not with the user's whole history.
        AiUsageScope::User(user_id) => {
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
                    ) AS "mapped!"
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
        AiUsageScope::Everyone => {
            sqlx::query_as!(
                ProjectKeyUse,
                r#"
                SELECT
                    bucket.project_key AS "project_key!",
                    (max(bucket.hour_start) AT TIME ZONE $1)::date AS "last_used_on!",
                    EXISTS (
                        SELECT 1 FROM ai_project_mappings AS mapping
                        WHERE mapping.project_key = bucket.project_key
                    ) AS "mapped!"
                FROM ai_usage_buckets AS bucket
                GROUP BY bucket.project_key
                ORDER BY 2 DESC, 1
                "#,
                time_zone.as_str(),
            )
            .fetch_all(pool)
            .await
        }
    }
}
