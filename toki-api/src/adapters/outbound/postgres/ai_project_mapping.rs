use async_trait::async_trait;
use time::OffsetDateTime;

use super::ai_usage_reads::project_key_usage;
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiMappedProject, AiProjectKey, AiProjectMapping, AiProjectMappingEditor, AiUsageScope,
            AiUsageTimeZone, ProjectId, TimeTrackingCompany, UnmappedAiProjectKey, UserId,
        },
        ports::outbound::AiProjectMappingRepository,
        AiProjectMappingError,
    },
};

pub struct PostgresAiProjectMappingRepository {
    pool: DbPool,
}

impl PostgresAiProjectMappingRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// A stored mapping with its editors' names and whether any usage has its key.
struct MappingRow {
    project_key: String,
    provider: String,
    provider_company_id: String,
    project_id: String,
    project_name: String,
    created_by: Option<i32>,
    created_by_name: Option<String>,
    created_at: OffsetDateTime,
    updated_by: Option<i32>,
    updated_by_name: Option<String>,
    updated_at: OffsetDateTime,
    in_use: bool,
}

#[async_trait]
impl AiProjectMappingRepository for PostgresAiProjectMappingRepository {
    async fn list(
        &self,
        scope: AiUsageScope,
    ) -> Result<Vec<AiProjectMapping>, AiProjectMappingError> {
        let rows = sqlx::query_as!(
            MappingRow,
            r#"
            SELECT
                mapping.project_key,
                mapping.provider,
                mapping.provider_company_id,
                mapping.project_id,
                mapping.project_name,
                mapping.created_by,
                creator.full_name AS "created_by_name?",
                mapping.created_at,
                mapping.updated_by,
                editor.full_name AS "updated_by_name?",
                mapping.updated_at,
                EXISTS (
                    SELECT 1 FROM ai_usage_buckets AS bucket
                    WHERE bucket.project_key = mapping.project_key
                ) AS "in_use!"
            FROM ai_project_mappings AS mapping
            LEFT JOIN users AS creator ON creator.id = mapping.created_by
            LEFT JOIN users AS editor ON editor.id = mapping.updated_by
            WHERE $1::int4 IS NULL
               OR EXISTS (
                   SELECT 1 FROM ai_usage_buckets AS bucket
                   WHERE bucket.project_key = mapping.project_key AND bucket.user_id = $1
               )
            ORDER BY mapping.project_key
            "#,
            scope_user(scope),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows.into_iter().map(AiProjectMapping::from).collect())
    }

    async fn list_unmapped(
        &self,
        scope: AiUsageScope,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<UnmappedAiProjectKey>, AiProjectMappingError> {
        let keys = project_key_usage(&self.pool, scope, time_zone)
            .await
            .map_err(storage_error)?;

        Ok(keys
            .into_iter()
            .filter(|key| !key.mapped)
            .map(|key| UnmappedAiProjectKey::new(key.project_key, key.last_used_on))
            .collect())
    }

    async fn appears_in_usage(
        &self,
        user_id: &UserId,
        project_key: &AiProjectKey,
    ) -> Result<bool, AiProjectMappingError> {
        sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM ai_usage_buckets WHERE project_key = $1 AND user_id = $2
            ) AS "appears!"
            "#,
            project_key.as_str(),
            user_id.as_i32(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }

    async fn create(
        &self,
        project_key: &AiProjectKey,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        created_by: &UserId,
    ) -> Result<Option<AiProjectMapping>, AiProjectMappingError> {
        let row = sqlx::query_as!(
            MappingRow,
            r#"
            WITH saved AS (
                INSERT INTO ai_project_mappings (
                    project_key, provider, provider_company_id, project_id, project_name,
                    created_by, updated_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, $6)
                ON CONFLICT (project_key) DO NOTHING
                RETURNING *
            )
            SELECT
                saved.project_key AS "project_key!",
                saved.provider AS "provider!",
                saved.provider_company_id AS "provider_company_id!",
                saved.project_id AS "project_id!",
                saved.project_name AS "project_name!",
                saved.created_by AS "created_by?",
                creator.full_name AS "created_by_name?",
                saved.created_at AS "created_at!",
                saved.updated_by AS "updated_by?",
                editor.full_name AS "updated_by_name?",
                saved.updated_at AS "updated_at!",
                EXISTS (
                    SELECT 1 FROM ai_usage_buckets AS bucket
                    WHERE bucket.project_key = saved.project_key
                ) AS "in_use!"
            FROM saved
            LEFT JOIN users AS creator ON creator.id = saved.created_by
            LEFT JOIN users AS editor ON editor.id = saved.updated_by
            "#,
            project_key.as_str(),
            company.provider,
            company.company_id,
            project.id.as_str(),
            project.name,
            created_by.as_i32(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(AiProjectMapping::from))
    }

    async fn upsert(
        &self,
        project_key: &AiProjectKey,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        updated_by: &UserId,
    ) -> Result<AiProjectMapping, AiProjectMappingError> {
        let row = sqlx::query_as!(
            MappingRow,
            r#"
            WITH saved AS (
                INSERT INTO ai_project_mappings (
                    project_key, provider, provider_company_id, project_id, project_name,
                    created_by, updated_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, $6)
                ON CONFLICT (project_key) DO UPDATE
                SET provider = EXCLUDED.provider,
                    provider_company_id = EXCLUDED.provider_company_id,
                    project_id = EXCLUDED.project_id,
                    project_name = EXCLUDED.project_name,
                    updated_by = EXCLUDED.updated_by,
                    updated_at = now()
                RETURNING *
            )
            SELECT
                saved.project_key AS "project_key!",
                saved.provider AS "provider!",
                saved.provider_company_id AS "provider_company_id!",
                saved.project_id AS "project_id!",
                saved.project_name AS "project_name!",
                saved.created_by AS "created_by?",
                creator.full_name AS "created_by_name?",
                saved.created_at AS "created_at!",
                saved.updated_by AS "updated_by?",
                editor.full_name AS "updated_by_name?",
                saved.updated_at AS "updated_at!",
                EXISTS (
                    SELECT 1 FROM ai_usage_buckets AS bucket
                    WHERE bucket.project_key = saved.project_key
                ) AS "in_use!"
            FROM saved
            LEFT JOIN users AS creator ON creator.id = saved.created_by
            LEFT JOIN users AS editor ON editor.id = saved.updated_by
            "#,
            project_key.as_str(),
            company.provider,
            company.company_id,
            project.id.as_str(),
            project.name,
            updated_by.as_i32(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.into())
    }

    async fn delete(&self, project_key: &AiProjectKey) -> Result<bool, AiProjectMappingError> {
        let deleted = sqlx::query!(
            "DELETE FROM ai_project_mappings WHERE project_key = $1",
            project_key.as_str(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(deleted.rows_affected() > 0)
    }
}

impl From<MappingRow> for AiProjectMapping {
    fn from(row: MappingRow) -> Self {
        Self {
            project_key: row.project_key,
            company: TimeTrackingCompany {
                provider: row.provider,
                company_id: row.provider_company_id,
            },
            project: AiMappedProject {
                id: ProjectId::new(row.project_id),
                name: row.project_name,
            },
            created_by: editor(row.created_by, row.created_by_name),
            created_at: row.created_at,
            updated_by: editor(row.updated_by, row.updated_by_name),
            updated_at: row.updated_at,
            in_use: row.in_use,
        }
    }
}

/// The user whose usage a query is limited to, or `None` for everyone's.
fn scope_user(scope: AiUsageScope) -> Option<i32> {
    match scope {
        AiUsageScope::User(user_id) => Some(user_id.as_i32()),
        AiUsageScope::Everyone => None,
    }
}

fn editor(id: Option<i32>, full_name: Option<String>) -> Option<AiProjectMappingEditor> {
    id.zip(full_name)
        .map(|(id, full_name)| AiProjectMappingEditor {
            id: UserId::new(id),
            full_name,
        })
}

fn storage_error(error: sqlx::Error) -> AiProjectMappingError {
    AiProjectMappingError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn the_longest_project_key_can_be_mapped(pool: PgPool) {
        let repository = PostgresAiProjectMappingRepository::new(
            sqlx_tracing::PoolBuilder::from(pool.clone()).build(),
        );
        let user: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ('admin@example.com', 'Test User', '', '')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // 512 varied four-byte characters, too irregular for Postgres to
        // compress: the largest primary-key entry an uploaded key can produce.
        let longest = (0..512u32)
            .map(|index| char::from_u32(0x10000 + index.wrapping_mul(2_654_435_761) % 0xF_0000))
            .collect::<Option<String>>()
            .unwrap();
        let project = AiMappedProject {
            id: ProjectId::new("101"),
            name: "Client A delivery".to_string(),
        };

        let company = TimeTrackingCompany {
            provider: "kleer".to_string(),
            company_id: "company-1".to_string(),
        };

        let saved = repository
            .upsert(
                &AiProjectKey::parse(&longest).unwrap(),
                &company,
                &project,
                &UserId::new(user),
            )
            .await
            .unwrap();

        assert_eq!(saved.project_key, longest);
    }
}
