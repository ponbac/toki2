use chrono::{DateTime, Utc};
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

    async fn get_user_avatar(&self, user_id: i32) -> Result<Option<UserAvatar>, RepositoryError>;

    async fn set_user_avatar(
        &self,
        user_id: i32,
        image: Vec<u8>,
        mime_type: String,
    ) -> Result<(), RepositoryError>;

    async fn clear_user_avatar(&self, user_id: i32) -> Result<(), RepositoryError>;

    async fn avatar_updated_at(
        &self,
        user_id: i32,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError>;

    async fn users_with_avatars_by_email(
        &self,
        emails: &[String],
    ) -> Result<Vec<UserAvatarIdentity>, RepositoryError>;
}

pub struct UserRepositoryImpl {
    pool: PgPool,
}

impl UserRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub struct UserAvatar {
    pub image: Vec<u8>,
    pub mime_type: String,
}

pub struct UserAvatarIdentity {
    pub user_id: i32,
    pub email: String,
    pub updated_at: DateTime<Utc>,
}

impl UserRepository for UserRepositoryImpl {
    async fn get_user(&self, id: i32) -> Result<User, RepositoryError> {
        let db_user = sqlx::query_as!(
            DbUser,
            r#"
            SELECT id, email, full_name, picture, access_token, roles
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
        };

        Ok(user)
    }

    async fn get_users(&self) -> Result<Vec<User>, RepositoryError> {
        let db_users = sqlx::query_as!(
            DbUser,
            r#"SELECT id, email, full_name, picture, access_token, roles FROM users"#
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
            })
            .collect();

        Ok(users)
    }

    async fn upsert_user(&self, user: &NewUser) -> Result<User, RepositoryError> {
        let role_strings: Vec<String> = user.roles.iter().map(|role| role.to_string()).collect();

        let db_user = sqlx::query_as!(
            DbUser,
            r#"
            INSERT INTO users (email, full_name, picture, access_token, roles)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(email) DO UPDATE
            SET full_name = EXCLUDED.full_name,
                picture = EXCLUDED.picture,
                access_token = EXCLUDED.access_token
            RETURNING id, email, full_name, picture, access_token, roles
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

    async fn get_user_avatar(&self, user_id: i32) -> Result<Option<UserAvatar>, RepositoryError> {
        let row = sqlx::query!(
            r#"
            SELECT image, mime_type
            FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| UserAvatar {
            image: row.image,
            mime_type: row.mime_type,
        }))
    }

    async fn set_user_avatar(
        &self,
        user_id: i32,
        image: Vec<u8>,
        mime_type: String,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            INSERT INTO user_avatars (user_id, image, mime_type, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (user_id) DO UPDATE
            SET image = EXCLUDED.image,
                mime_type = EXCLUDED.mime_type,
                updated_at = now(),
                created_at = now()
            "#,
            user_id,
            image,
            mime_type
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn clear_user_avatar(&self, user_id: i32) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"
            DELETE FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn avatar_updated_at(
        &self,
        user_id: i32,
    ) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        let row = sqlx::query!(
            r#"
            SELECT updated_at as "updated_at: chrono::DateTime<Utc>"
            FROM user_avatars
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| row.updated_at))
    }

    async fn users_with_avatars_by_email(
        &self,
        emails: &[String],
    ) -> Result<Vec<UserAvatarIdentity>, RepositoryError> {
        if emails.is_empty() {
            return Ok(vec![]);
        }

        let rows = sqlx::query!(
            r#"
            SELECT users.id, users.email, user_avatars.updated_at as "updated_at: chrono::DateTime<Utc>"
            FROM users
            INNER JOIN user_avatars ON user_avatars.user_id = users.id
            WHERE users.email = ANY($1)
            "#,
            emails
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| UserAvatarIdentity {
                user_id: row.id,
                email: row.email,
                updated_at: row.updated_at,
            })
            .collect())
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
}
