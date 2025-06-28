use sqlx::PgPool;

use crate::domain::{RepoKey, Repository};

use super::repo_error::RepositoryError;

pub trait RepoRepository {
    async fn get_repositories(&self) -> Result<Vec<Repository>, RepositoryError>;
    async fn upsert_repository(&self, repository: &NewRepository) -> Result<i32, RepositoryError>;
    async fn delete_repository(&self, repo_key: &RepoKey) -> Result<(), RepositoryError>;
    async fn update_milltime_projects(&self, repo_key: &RepoKey, milltime_project_ids: Vec<String>) -> Result<(), RepositoryError>;
}

pub struct RepoRepositoryImpl {
    pool: PgPool,
}

impl RepoRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl RepoRepository for RepoRepositoryImpl {
    async fn get_repositories(&self) -> Result<Vec<Repository>, RepositoryError> {
        let repos = sqlx::query!(
            r#"
            SELECT r.id, r.organization, r.project, r.repo_name,
                   ARRAY_AGG(rmp.milltime_project_id) FILTER (WHERE rmp.milltime_project_id IS NOT NULL) as milltime_project_ids
            FROM repositories r
            LEFT JOIN repository_milltime_projects rmp ON r.id = rmp.repository_id
            GROUP BY r.id, r.organization, r.project, r.repo_name
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(repos
            .into_iter()
            .map(|r| Repository {
                id: r.id,
                organization: r.organization,
                project: r.project,
                repo_name: r.repo_name,
                milltime_project_ids: r.milltime_project_ids.unwrap_or_default(),
            })
            .collect())
    }

    async fn upsert_repository(&self, repository: &NewRepository) -> Result<i32, RepositoryError> {
        let id = sqlx::query!(
            r#"
            INSERT INTO repositories (organization, project, repo_name, token)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(organization, project, repo_name) DO UPDATE
            SET token = EXCLUDED.token
            RETURNING id
            "#,
            repository.organization,
            repository.project,
            repository.repo_name,
            repository.token
        )
        .fetch_one(&self.pool)
        .await?
        .id;

        Ok(id)
    }

    async fn delete_repository(&self, repo_key: &RepoKey) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            DELETE FROM repositories
            WHERE organization = $1 AND project = $2 AND repo_name = $3
            "#,
            repo_key.organization,
            repo_key.project,
            repo_key.repo_name,
        )
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::from)?;

        Ok(())
    }

    async fn update_milltime_projects(&self, repo_key: &RepoKey, milltime_project_ids: Vec<String>) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await?;

        // Get repository ID
        let repo = sqlx::query!(
            r#"
            SELECT id FROM repositories
            WHERE organization = $1 AND project = $2 AND repo_name = $3
            "#,
            repo_key.organization,
            repo_key.project,
            repo_key.repo_name,
        )
        .fetch_one(&mut *tx)
        .await?;

        // Delete existing connections
        sqlx::query!(
            r#"
            DELETE FROM repository_milltime_projects
            WHERE repository_id = $1
            "#,
            repo.id,
        )
        .execute(&mut *tx)
        .await?;

        // Insert new connections
        for project_id in milltime_project_ids {
            sqlx::query!(
                r#"
                INSERT INTO repository_milltime_projects (repository_id, milltime_project_id)
                VALUES ($1, $2)
                "#,
                repo.id,
                project_id,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

pub struct NewRepository {
    organization: String,
    project: String,
    repo_name: String,
    token: String,
}

impl NewRepository {
    pub fn new(organization: String, project: String, repo_name: String, token: String) -> Self {
        Self {
            organization,
            project,
            repo_name,
            token,
        }
    }
}
