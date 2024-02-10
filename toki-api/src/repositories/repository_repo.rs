use sqlx::PgPool;

use crate::domain::Repository;

use super::repo_error::RepositoryError;

pub trait RepoRepository {
    async fn get_repositories(&self) -> Result<Vec<Repository>, RepositoryError>;
    async fn upsert_repository(&self, repository: &NewRepository) -> Result<i32, RepositoryError>;
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
            SELECT id, organization, project, repo_name
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
