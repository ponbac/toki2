use sqlx::PgPool;

use crate::domain::RepoKey;

use super::repo_error::RepositoryError;

pub trait UserRepository {
    async fn followed_repositories(&self, id: i32) -> Result<Vec<RepoKey>, RepositoryError>;
    async fn follow_repository(
        &self,
        user_id: i32,
        repo: RepoKey,
        follow: bool,
    ) -> Result<(), RepositoryError>;
}

pub struct UserRepositoryImpl {
    pool: PgPool,
}

impl UserRepository for UserRepositoryImpl {
    async fn followed_repositories(&self, id: i32) -> Result<Vec<RepoKey>, RepositoryError> {
        let repos = sqlx::query_as!(
            RepoKey,
            r#"
            SELECT organization, project, repo_name
            FROM user_repositories
            JOIN repositories ON user_repositories.repository_id = repositories.id
            WHERE user_id = $1
            "#,
            id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(repos)
    }

    async fn follow_repository(
        &self,
        user_id: i32,
        repo: RepoKey,
        follow: bool,
    ) -> Result<(), RepositoryError> {
        let repo_id = sqlx::query!(
            r#"
            SELECT id
            FROM repositories
            WHERE organization = $1 AND project = $2 AND repo_name = $3
            "#,
            repo.organization,
            repo.project,
            repo.repo_name
        )
        .fetch_one(&self.pool)
        .await?
        .id;

        if follow {
            sqlx::query!(
                r#"
                INSERT INTO user_repositories (user_id, repository_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
                user_id,
                repo_id
            )
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query!(
                r#"
                DELETE FROM user_repositories
                WHERE user_id = $1 AND repository_id = $2
                "#,
                user_id,
                repo_id
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}
