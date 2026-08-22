use async_trait::async_trait;

use crate::{
    db::DbPool,
    domain::{
        models::{
            ApiToken, ApiTokenCapabilities, ApiTokenGrant, ApiTokenHash, ApiTokenId, ApiTokenName,
            NewApiToken, UserId,
        },
        ports::outbound::ApiTokenRepository,
        ApiTokenError, Role, UserPrincipal,
    },
};

pub struct PostgresApiTokenRepository {
    pool: DbPool,
}

impl PostgresApiTokenRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiTokenRepository for PostgresApiTokenRepository {
    async fn insert_if_below_limit(
        &self,
        user_id: &UserId,
        token: &NewApiToken,
        max_tokens: usize,
    ) -> Result<ApiToken, ApiTokenError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Serialize issuance for one user so the token cap remains an invariant
        // under concurrent requests.
        sqlx::query!(
            r#"
            SELECT id
            FROM users
            WHERE id = $1
            FOR UPDATE
            "#,
            user_id.as_i32(),
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        let token_count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM api_tokens
            WHERE user_id = $1
            "#,
            user_id.as_i32(),
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        let max_tokens = i64::try_from(max_tokens)
            .map_err(|_| ApiTokenError::Storage("token limit is out of range".to_string()))?;
        if token_count >= max_tokens {
            return Err(ApiTokenError::TooManyTokens);
        }

        let capabilities = token.capabilities().as_strings();
        let row = sqlx::query!(
            r#"
            INSERT INTO api_tokens (user_id, name, token_prefix, token_hash, capabilities)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, user_id, name, token_prefix, capabilities, created_at
            "#,
            user_id.as_i32(),
            token.name().as_str(),
            token.prefix(),
            token.hash().as_bytes().as_slice(),
            &capabilities,
        )
        .fetch_one(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;

        Ok(ApiToken {
            id: ApiTokenId::new(row.id),
            user_id: UserId::from(row.user_id),
            name: stored_name(row.name)?,
            prefix: row.token_prefix,
            capabilities: stored_capabilities(row.capabilities)?,
            created_at: row.created_at,
        })
    }

    async fn list_for_user(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, name, token_prefix, capabilities, created_at
            FROM api_tokens
            WHERE user_id = $1
            ORDER BY created_at DESC, id DESC
            "#,
            user_id.as_i32(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                Ok(ApiToken {
                    id: ApiTokenId::new(row.id),
                    user_id: UserId::from(row.user_id),
                    name: stored_name(row.name)?,
                    prefix: row.token_prefix,
                    capabilities: stored_capabilities(row.capabilities)?,
                    created_at: row.created_at,
                })
            })
            .collect::<Result<Vec<_>, ApiTokenError>>()?)
    }

    async fn delete_for_user(
        &self,
        user_id: &UserId,
        token_id: &ApiTokenId,
    ) -> Result<(), ApiTokenError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM api_tokens
            WHERE id = $1 AND user_id = $2
            "#,
            token_id.as_i32(),
            user_id.as_i32(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(ApiTokenError::NotFound);
        }

        Ok(())
    }

    async fn find_grant_by_token_hash(
        &self,
        hash: &ApiTokenHash,
    ) -> Result<Option<ApiTokenGrant>, ApiTokenError> {
        let row = sqlx::query!(
            r#"
            SELECT u.id, u.email, u.roles, t.capabilities
            FROM api_tokens t
            INNER JOIN users u ON u.id = t.user_id
            WHERE t.token_hash = $1
            "#,
            hash.as_bytes().as_slice(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| {
            Ok(ApiTokenGrant {
                principal: UserPrincipal {
                    id: UserId::from(row.id),
                    email: row.email,
                    roles: row.roles.into_iter().map(Role::from).collect(),
                },
                capabilities: stored_capabilities(row.capabilities)?,
            })
        })
        .transpose()
    }
}

fn stored_name(name: String) -> Result<ApiTokenName, ApiTokenError> {
    ApiTokenName::parse(&name)
        .ok_or_else(|| ApiTokenError::Storage("stored API token name is invalid".to_string()))
}

fn stored_capabilities(capabilities: Vec<String>) -> Result<ApiTokenCapabilities, ApiTokenError> {
    ApiTokenCapabilities::parse(capabilities).map_err(|_| {
        ApiTokenError::Storage("stored API token capabilities are invalid".to_string())
    })
}

fn storage_error(error: sqlx::Error) -> ApiTokenError {
    ApiTokenError::Storage(error.to_string())
}
