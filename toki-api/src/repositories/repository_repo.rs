use sqlx::PgPool;

use crate::domain::{RepoKey, Repository};

use super::repo_error::RepositoryError;

pub trait RepoRepository {
    async fn get_repositories(&self) -> Result<Vec<Repository>, RepositoryError>;
    async fn upsert_repository(&self, repository: &NewRepository) -> Result<i32, RepositoryError>;
    async fn delete_repository(&self, repo_key: &RepoKey) -> Result<(), RepositoryError>;
    async fn update_milltime_project(&self, repo_key: &RepoKey, milltime_project_id: Option<String>) -> Result<(), RepositoryError>;
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
        let repos = sqlx::query_as!(
            Repository,
            r#"
            SELECT id, organization, project, repo_name, milltime_project_id
            FROM repositories
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(repos)
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

    async fn update_milltime_project(&self, repo_key: &RepoKey, milltime_project_id: Option<String>) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            UPDATE repositories
            SET milltime_project_id = $4
            WHERE organization = $1 AND project = $2 AND repo_name = $3
            "#,
            repo_key.organization,
            repo_key.project,
            repo_key.repo_name,
            milltime_project_id,
        )
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::from)?;

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
