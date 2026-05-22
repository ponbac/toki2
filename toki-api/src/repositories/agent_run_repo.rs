use sqlx::PgPool;
use time::OffsetDateTime;

use crate::routes::agent_runs::AgentRunRecord;

use super::RepositoryError;

pub const AZURE_DEVOPS_WORK_ITEM_SOURCE_PROVIDER: &str = "azureDevOpsWorkItem";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AgentRunWorkItemRow {
    pub run_id: String,
    pub source_provider: String,
    pub source_organization: String,
    pub source_project: String,
    pub source_work_item_id: String,
    pub target_provider: String,
    pub target_organization: Option<String>,
    pub target_project: Option<String>,
    pub target_repo_name: Option<String>,
    pub target_default_branch: Option<String>,
    pub created_by_user_id: Option<i32>,
    pub created_by_display_name: String,
    pub last_status: String,
    pub draft_pr_url: Option<String>,
    pub run_created_at: OffsetDateTime,
    pub run_updated_at: OffsetDateTime,
    pub last_synced_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct AgentRunIssueSummary {
    pub run_id: String,
    pub source_work_item_id: String,
    pub last_status: String,
    pub draft_pr_url: Option<String>,
    pub created_by_display_name: String,
    pub run_created_at: OffsetDateTime,
    pub run_updated_at: OffsetDateTime,
    pub last_synced_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct AdoWorkItemSourceMetadata {
    pub organization: String,
    pub project: String,
    pub work_item_id: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunActorMetadata {
    pub user_id: i32,
    pub display_name: String,
}

pub struct AgentRunRepositoryImpl {
    pool: PgPool,
}

impl AgentRunRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_from_run(
        &self,
        run: &AgentRunRecord,
        source: &AdoWorkItemSourceMetadata,
        actor: &AgentRunActorMetadata,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            INSERT INTO agent_run_work_items (
                run_id,
                source_provider,
                source_organization,
                source_project,
                source_work_item_id,
                target_provider,
                target_organization,
                target_project,
                target_repo_name,
                target_default_branch,
                created_by_user_id,
                created_by_display_name,
                last_status,
                draft_pr_url,
                run_created_at,
                run_updated_at,
                last_synced_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, now())
            ON CONFLICT (run_id) DO UPDATE
            SET source_provider = EXCLUDED.source_provider,
                source_organization = EXCLUDED.source_organization,
                source_project = EXCLUDED.source_project,
                source_work_item_id = EXCLUDED.source_work_item_id,
                target_provider = EXCLUDED.target_provider,
                target_organization = EXCLUDED.target_organization,
                target_project = EXCLUDED.target_project,
                target_repo_name = EXCLUDED.target_repo_name,
                target_default_branch = EXCLUDED.target_default_branch,
                created_by_user_id = EXCLUDED.created_by_user_id,
                created_by_display_name = EXCLUDED.created_by_display_name,
                last_status = EXCLUDED.last_status,
                draft_pr_url = EXCLUDED.draft_pr_url,
                run_created_at = EXCLUDED.run_created_at,
                run_updated_at = EXCLUDED.run_updated_at,
                last_synced_at = now()
            "#,
            run.id.as_str(),
            AZURE_DEVOPS_WORK_ITEM_SOURCE_PROVIDER,
            source.organization.as_str(),
            source.project.as_str(),
            source.work_item_id.as_str(),
            run.target_repo.provider.to_string(),
            run.target_repo.organization.as_deref(),
            run.target_repo.project.as_deref(),
            run.target_repo.repo_name.as_deref(),
            run.target_repo.default_branch.as_str(),
            actor.user_id,
            actor.display_name.as_str(),
            run.status.as_str(),
            run.draft_pr_url(),
            run.created_at,
            run.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_by_run_id(
        &self,
        run_id: &str,
    ) -> Result<Option<AgentRunWorkItemRow>, RepositoryError> {
        let row = sqlx::query_as!(
            AgentRunWorkItemRow,
            r#"
            SELECT
                run_id,
                source_provider,
                source_organization,
                source_project,
                source_work_item_id,
                target_provider,
                target_organization,
                target_project,
                target_repo_name,
                target_default_branch,
                created_by_user_id,
                created_by_display_name,
                last_status,
                draft_pr_url,
                run_created_at,
                run_updated_at,
                last_synced_at
            FROM agent_run_work_items
            WHERE run_id = $1
            "#,
            run_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_latest_by_work_items(
        &self,
        source_provider: &str,
        organization: &str,
        project: &str,
        work_item_ids: &[String],
    ) -> Result<Vec<AgentRunWorkItemRow>, RepositoryError> {
        let rows = sqlx::query_as!(
            AgentRunWorkItemRow,
            r#"
            SELECT DISTINCT ON (source_work_item_id)
                run_id,
                source_provider,
                source_organization,
                source_project,
                source_work_item_id,
                target_provider,
                target_organization,
                target_project,
                target_repo_name,
                target_default_branch,
                created_by_user_id,
                created_by_display_name,
                last_status,
                draft_pr_url,
                run_created_at,
                run_updated_at,
                last_synced_at
            FROM agent_run_work_items
            WHERE source_provider = $1
              AND source_organization = $2
              AND source_project = $3
              AND source_work_item_id = ANY($4)
            ORDER BY source_work_item_id, run_created_at DESC
            "#,
            source_provider,
            organization,
            project,
            work_item_ids
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn update_synced_summary(
        &self,
        run_id: &str,
        status: &str,
        draft_pr_url: Option<&str>,
        run_updated_at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE agent_run_work_items
            SET last_status = $2,
                draft_pr_url = $3,
                run_updated_at = $4,
                last_synced_at = now()
            WHERE run_id = $1
            "#,
            run_id,
            status,
            draft_pr_url,
            run_updated_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_by_run_id(&self, run_id: &str) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM agent_run_work_items WHERE run_id = $1")
            .bind(run_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

impl From<AgentRunWorkItemRow> for AgentRunIssueSummary {
    fn from(row: AgentRunWorkItemRow) -> Self {
        Self {
            run_id: row.run_id,
            source_work_item_id: row.source_work_item_id,
            last_status: row.last_status,
            draft_pr_url: row.draft_pr_url,
            created_by_display_name: row.created_by_display_name,
            run_created_at: row.run_created_at,
            run_updated_at: row.run_updated_at,
            last_synced_at: row.last_synced_at,
        }
    }
}
