//! The mappings of a metered payer's repositories in Postgres
//! (`ai_provider_repository_mappings`), matched case-insensitively through
//! the unique index on `(provider, lower(repository_name))`.

use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use super::ai_usage_reads::stored_mappings;
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiMappedProject, AiProjectMappingEditor, AiProvider, AiRepositoryDayCredits,
            AiRepositoryMapping, AiRepositoryName, AiStoredMapping, AiUsdNanos, ProjectId,
            TimeTrackingCompany, UserId,
        },
        ports::outbound::AiRepositoryMappingRepository,
        AiRepositoryMappingError,
    },
};

pub struct PostgresAiRepositoryMappingRepository {
    pool: DbPool,
}

impl PostgresAiRepositoryMappingRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// A stored mapping with its editors' names.
struct MappingRow {
    provider: String,
    repository_name: String,
    time_tracking_provider: String,
    provider_company_id: String,
    project_id: String,
    project_name: String,
    created_by: Option<i32>,
    created_by_name: Option<String>,
    created_at: OffsetDateTime,
    updated_by: Option<i32>,
    updated_by_name: Option<String>,
    updated_at: OffsetDateTime,
}

impl MappingRow {
    fn into_mapping(self) -> Result<AiRepositoryMapping, AiRepositoryMappingError> {
        Ok(AiRepositoryMapping {
            payer: AiProvider::parse(&self.provider).ok_or_else(|| {
                AiRepositoryMappingError::Storage(format!(
                    "corrupt repository mapping provider {:?}",
                    self.provider
                ))
            })?,
            repository: self.repository_name,
            company: TimeTrackingCompany {
                provider: self.time_tracking_provider,
                company_id: self.provider_company_id,
            },
            project: AiMappedProject {
                id: ProjectId::new(self.project_id),
                name: self.project_name,
            },
            created_by: editor(self.created_by, self.created_by_name),
            created_at: self.created_at,
            updated_by: editor(self.updated_by, self.updated_by_name),
            updated_at: self.updated_at,
        })
    }
}

fn editor(id: Option<i32>, full_name: Option<String>) -> Option<AiProjectMappingEditor> {
    id.zip(full_name)
        .map(|(id, full_name)| AiProjectMappingEditor {
            id: UserId::new(id),
            full_name,
        })
}

fn storage_error(error: sqlx::Error) -> AiRepositoryMappingError {
    AiRepositoryMappingError::Storage(error.to_string())
}

#[async_trait]
impl AiRepositoryMappingRepository for PostgresAiRepositoryMappingRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiRepositoryMappingError> {
        sqlx::query_scalar!(r#"SELECT now() AS "now!""#)
            .fetch_one(&self.pool)
            .await
            .map_err(storage_error)
    }

    async fn list(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryMapping>, AiRepositoryMappingError> {
        sqlx::query_as!(
            MappingRow,
            r#"
            SELECT
                mapping.provider,
                mapping.repository_name,
                mapping.time_tracking_provider,
                mapping.provider_company_id,
                mapping.project_id,
                mapping.project_name,
                mapping.created_by,
                creator.full_name AS "created_by_name?",
                mapping.created_at,
                mapping.updated_by,
                editor.full_name AS "updated_by_name?",
                mapping.updated_at
            FROM ai_provider_repository_mappings AS mapping
            LEFT JOIN users AS creator ON creator.id = mapping.created_by
            LEFT JOIN users AS editor ON editor.id = mapping.updated_by
            WHERE mapping.provider = $1
            ORDER BY lower(mapping.repository_name)
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(MappingRow::into_mapping)
        .collect()
    }

    async fn repository_credits(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryDayCredits>, AiRepositoryMappingError> {
        struct Row {
            repository: String,
            day: Date,
            credits: i64,
        }
        sqlx::query_as!(
            Row,
            r#"
            SELECT repository, day, sum(credit_usd_nanos)::int8 AS "credits!"
            FROM ai_provider_org_usage
            WHERE provider = $1 AND kind = 'repository_credits'
            GROUP BY repository, day
            ORDER BY day, repository
            "#,
            payer.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?
        .into_iter()
        .map(|row| {
            Ok(AiRepositoryDayCredits {
                credits: AiUsdNanos::new(row.credits).ok_or_else(|| {
                    AiRepositoryMappingError::Storage(
                        "a stored repository credit is negative".to_string(),
                    )
                })?,
                repository: row.repository,
                day: row.day,
            })
        })
        .collect()
    }

    async fn key_mappings(&self) -> Result<Vec<AiStoredMapping>, AiRepositoryMappingError> {
        stored_mappings(&self.pool).await.map_err(storage_error)
    }

    async fn upsert(
        &self,
        payer: AiProvider,
        repository: &str,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        by: &UserId,
    ) -> Result<AiRepositoryMapping, AiRepositoryMappingError> {
        sqlx::query_as!(
            MappingRow,
            r#"
            WITH saved AS (
                INSERT INTO ai_provider_repository_mappings (
                    provider, repository_name, time_tracking_provider, provider_company_id,
                    project_id, project_name, created_by, updated_by
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
                ON CONFLICT (provider, lower(repository_name)) DO UPDATE
                SET repository_name = EXCLUDED.repository_name,
                    time_tracking_provider = EXCLUDED.time_tracking_provider,
                    provider_company_id = EXCLUDED.provider_company_id,
                    project_id = EXCLUDED.project_id,
                    project_name = EXCLUDED.project_name,
                    updated_by = EXCLUDED.updated_by,
                    updated_at = now()
                RETURNING *
            )
            SELECT
                saved.provider AS "provider!",
                saved.repository_name AS "repository_name!",
                saved.time_tracking_provider AS "time_tracking_provider!",
                saved.provider_company_id AS "provider_company_id!",
                saved.project_id AS "project_id!",
                saved.project_name AS "project_name!",
                saved.created_by AS "created_by?",
                creator.full_name AS "created_by_name?",
                saved.created_at AS "created_at!",
                saved.updated_by AS "updated_by?",
                editor.full_name AS "updated_by_name?",
                saved.updated_at AS "updated_at!"
            FROM saved
            LEFT JOIN users AS creator ON creator.id = saved.created_by
            LEFT JOIN users AS editor ON editor.id = saved.updated_by
            "#,
            payer.as_str(),
            repository,
            company.provider,
            company.company_id,
            project.id.as_str(),
            project.name,
            by.as_i32(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?
        .into_mapping()
    }

    async fn delete(
        &self,
        payer: AiProvider,
        repository: &AiRepositoryName,
    ) -> Result<bool, AiRepositoryMappingError> {
        let deleted = sqlx::query!(
            r#"
            DELETE FROM ai_provider_repository_mappings
            WHERE provider = $1 AND lower(repository_name) = lower($2)
            "#,
            payer.as_str(),
            repository.as_str(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(deleted.rows_affected() > 0)
    }
}

/// The store with its clock pinned at `now`, so the months a test's overview
/// shows never follow the real date.
#[cfg(test)]
pub(crate) struct ClockedAiRepositoryMappingRepository {
    pub inner: PostgresAiRepositoryMappingRepository,
    pub now: OffsetDateTime,
}

#[cfg(test)]
#[async_trait]
impl AiRepositoryMappingRepository for ClockedAiRepositoryMappingRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiRepositoryMappingError> {
        Ok(self.now)
    }

    async fn list(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryMapping>, AiRepositoryMappingError> {
        self.inner.list(payer).await
    }

    async fn repository_credits(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryDayCredits>, AiRepositoryMappingError> {
        self.inner.repository_credits(payer).await
    }

    async fn key_mappings(&self) -> Result<Vec<AiStoredMapping>, AiRepositoryMappingError> {
        self.inner.key_mappings().await
    }

    async fn upsert(
        &self,
        payer: AiProvider,
        repository: &str,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        by: &UserId,
    ) -> Result<AiRepositoryMapping, AiRepositoryMappingError> {
        self.inner
            .upsert(payer, repository, company, project, by)
            .await
    }

    async fn delete(
        &self,
        payer: AiProvider,
        repository: &AiRepositoryName,
    ) -> Result<bool, AiRepositoryMappingError> {
        self.inner.delete(payer, repository).await
    }
}

#[cfg(test)]
mod tests;
