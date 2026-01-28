use sqlx::PgPool;

use crate::domain::{RepoKey, Role, User};

use super::repo_error::RepositoryError;

pub trait UserRepository {
    async fn get_user(&self, id: i32) -> Result<User, RepositoryError>;
    async fn get_users(&self) -> Result<Vec<User>, RepositoryError>;
    async fn upsert_user(&self, user: &NewUser) -> Result<User, RepositoryError>;
    async fn followed_repositories(&self, id: &i32) -> Result<Vec<RepoKey>, RepositoryError>;
    async fn follow_repository(
        &self,
        user_id: i32,
        repo: &RepoKey,
        follow: bool,
    ) -> Result<(), RepositoryError>;
}

pub struct UserRepositoryImpl {
    pool: PgPool,
}

impl UserRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for UserRepositoryImpl {
    async fn get_user(&self, id: i32) -> Result<User, RepositoryError> {
        let db_user = sqlx::query_as!(
            DbUser,
            r#"
            SELECT id, email, full_name, picture, access_token, roles, session_auth_hash
            FROM users
            WHERE id = $1
            "#,
            id
        )
        .fetch_one(&self.pool)
        .await?;

        let user = User {
            id: db_user.id,
            email: db_user.email,
            full_name: db_user.full_name,
            picture: db_user.picture,
            access_token: db_user.access_token,
            roles: db_user.roles.into_iter().map(Role::from).collect(),
            session_auth_hash: db_user.session_auth_hash,
        };

        Ok(user)
    }

    async fn get_users(&self) -> Result<Vec<User>, RepositoryError> {
        let db_users = sqlx::query_as!(
            DbUser,
            r#"SELECT id, email, full_name, picture, access_token, roles, session_auth_hash FROM users"#
        )
        .fetch_all(&self.pool)
        .await?;

        let users = db_users
            .into_iter()
            .map(|db_user| User {
                id: db_user.id,
                email: db_user.email,
                full_name: db_user.full_name,
                picture: db_user.picture,
                access_token: db_user.access_token,
                roles: db_user.roles.into_iter().map(Role::from).collect(),
                session_auth_hash: db_user.session_auth_hash,
            })
            .collect();

        Ok(users)
    }

    async fn upsert_user(&self, user: &NewUser) -> Result<User, RepositoryError> {
        let role_strings: Vec<String> = user.roles.iter().map(|role| role.to_string()).collect();

        // Note: session_auth_hash is intentionally NOT updated on conflict
        // This allows multiple devices to stay logged in when access_token changes
        let db_user = sqlx::query_as!(
            DbUser,
            r#"
            INSERT INTO users (email, full_name, picture, access_token, roles)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(email) DO UPDATE
            SET full_name = EXCLUDED.full_name,
                picture = EXCLUDED.picture,
                access_token = EXCLUDED.access_token
            RETURNING id, email, full_name, picture, access_token, roles, session_auth_hash
            "#,
            user.email,
            user.full_name,
            user.picture,
            user.access_token,
            &role_strings // Convert Vec<Role> to Vec<String>
        )
        .fetch_one(&self.pool)
        .await?;

        let user = User {
            id: db_user.id,
            email: db_user.email,
            full_name: db_user.full_name,
            picture: db_user.picture,
            access_token: db_user.access_token,
            roles: db_user.roles.into_iter().map(Role::from).collect(),
            session_auth_hash: db_user.session_auth_hash,
        };

        Ok(user)
    }

    async fn followed_repositories(&self, id: &i32) -> Result<Vec<RepoKey>, RepositoryError> {
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
        repo: &RepoKey,
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

pub struct NewUser {
    email: String,
    full_name: String,
    picture: String,
    access_token: String,
    roles: Vec<Role>,
}

impl NewUser {
    pub fn new(email: String, full_name: String, picture: String, access_token: String) -> Self {
        Self {
            email,
            full_name,
            picture,
            access_token,
            roles: vec![Role::User],
        }
    }
}

struct DbUser {
    id: i32,
    email: String,
    full_name: String,
    picture: String,
    access_token: String,
    roles: Vec<String>,
    session_auth_hash: String,
}
