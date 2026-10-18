use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    db::{DbPool, DbTransaction},
    domain::{
        models::{
            AccessTokenGrant, CodeDecision, CodeRedemption, DecidedAuthorizationRequest,
            NewAuthorizationCode, NewAuthorizationRequest, NewRegisteredClient, NewTokenPair,
            OAuthClient, OAuthClientKind, OAuthGrant, OAuthGrantId, OAuthSecretHash, PkceChallenge,
            RedirectUri, RefreshDecision, RefreshRotation, RegisteredClient,
            StoredAuthorizationCode, StoredRefreshToken, UserId,
        },
        ports::outbound::{CodeJudge, OAuthRepository, RefreshJudge},
        OAuthError, Role, UserPrincipal,
    },
};

pub struct PostgresOAuthRepository {
    pool: DbPool,
}

impl PostgresOAuthRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OAuthRepository for PostgresOAuthRepository {
    async fn find_registered_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuthClient>, OAuthError> {
        let row = sqlx::query!(
            r#"
            SELECT client_id, client_name, redirect_uris
            FROM oauth_clients
            WHERE client_id = $1
            "#,
            client_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        row.map(|row| registered_client(row.client_id, row.client_name, &row.redirect_uris))
            .transpose()
    }

    async fn insert_registered_client(
        &self,
        client: &NewRegisteredClient,
    ) -> Result<RegisteredClient, OAuthError> {
        let redirect_uris = client
            .redirect_uris
            .iter()
            .map(|uri| uri.as_str().to_string())
            .collect::<Vec<_>>();
        let row = sqlx::query!(
            r#"
            INSERT INTO oauth_clients (client_id, client_name, redirect_uris)
            VALUES ($1, $2, $3)
            RETURNING client_id, client_name, redirect_uris, created_at
            "#,
            client.id,
            client.name,
            &redirect_uris,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(RegisteredClient {
            client: registered_client(row.client_id, row.client_name, &row.redirect_uris)?,
            created_at: row.created_at,
        })
    }

    async fn insert_authorization_request(
        &self,
        request: &NewAuthorizationRequest,
    ) -> Result<(), OAuthError> {
        sqlx::query!(
            r#"
            INSERT INTO oauth_authorization_requests
                (id_hash, user_id, client_id, client_name, redirect_uri, resource,
                 code_challenge, state, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            request.id.as_bytes().as_slice(),
            request.user_id.as_i32(),
            request.client_id,
            request.client_name,
            request.redirect_uri,
            request.resource,
            request.code_challenge.as_str(),
            request.state,
            request.expires_at,
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(())
    }

    async fn take_authorization_request(
        &self,
        id: &OAuthSecretHash,
        user_id: &UserId,
        code: Option<&NewAuthorizationCode>,
    ) -> Result<Option<DecidedAuthorizationRequest>, OAuthError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Deleting the row is what decides the request: of concurrent
        // decisions, only the one whose DELETE returns it proceeds.
        let Some(request) = sqlx::query!(
            r#"
            DELETE FROM oauth_authorization_requests
            WHERE id_hash = $1 AND user_id = $2 AND expires_at > now()
            RETURNING user_id, client_id, client_name, redirect_uri, resource,
                      code_challenge, state
            "#,
            id.as_bytes().as_slice(),
            user_id.as_i32(),
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        else {
            return Ok(None);
        };

        if let Some(code) = code {
            sqlx::query!(
                r#"
                INSERT INTO oauth_authorization_codes
                    (code_hash, user_id, client_id, client_name, redirect_uri, resource,
                     code_challenge, expires_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
                code.hash.as_bytes().as_slice(),
                request.user_id,
                request.client_id,
                request.client_name,
                request.redirect_uri,
                request.resource,
                request.code_challenge,
                code.expires_at,
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        }

        transaction.commit().await.map_err(storage_error)?;

        Ok(Some(DecidedAuthorizationRequest {
            redirect_uri: request.redirect_uri,
            state: request.state,
        }))
    }

    async fn redeem_authorization_code(
        &self,
        code: &OAuthSecretHash,
        judge: &CodeJudge,
        grant_expires_at: OffsetDateTime,
        tokens: &NewTokenPair,
    ) -> Result<CodeRedemption, OAuthError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Concurrent redemptions of one code queue on this lock, so each sees
        // whether an earlier one consumed the code and which grant it made.
        let Some(row) = sqlx::query!(
            r#"
            SELECT user_id, client_id, client_name, redirect_uri, resource, code_challenge,
                   expires_at, consumed_at IS NOT NULL AS "consumed!", grant_id
            FROM oauth_authorization_codes
            WHERE code_hash = $1
            FOR UPDATE
            "#,
            code.as_bytes().as_slice(),
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        else {
            return Ok(CodeRedemption::Unknown);
        };

        let stored = StoredAuthorizationCode {
            user_id: UserId::from(row.user_id),
            client_id: row.client_id,
            client_name: row.client_name,
            redirect_uri: row.redirect_uri,
            resource: row.resource,
            code_challenge: stored_challenge(&row.code_challenge)?,
            expires_at: row.expires_at,
            consumed: row.consumed,
        };

        let redemption = match judge(&stored) {
            CodeDecision::Redeem => {
                if stored.consumed {
                    return Err(OAuthError::Storage(
                        "a consumed authorization code cannot be redeemed".to_string(),
                    ));
                }
                let grant_id = sqlx::query_scalar!(
                    r#"
                    INSERT INTO oauth_grants
                        (user_id, client_id, client_name, redirect_uri, resource, expires_at)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    RETURNING id
                    "#,
                    stored.user_id.as_i32(),
                    stored.client_id,
                    stored.client_name,
                    stored.redirect_uri,
                    stored.resource,
                    grant_expires_at,
                )
                .fetch_one(&mut transaction.executor())
                .await
                .map_err(storage_error)?;

                sqlx::query!(
                    r#"
                    UPDATE oauth_authorization_codes
                    SET consumed_at = now(), grant_id = $2
                    WHERE code_hash = $1
                    "#,
                    code.as_bytes().as_slice(),
                    grant_id,
                )
                .execute(&mut transaction.executor())
                .await
                .map_err(storage_error)?;

                // A metadata-document client has no row, so this does nothing
                // for one.
                sqlx::query!(
                    r#"
                    UPDATE oauth_clients
                    SET last_used_at = now()
                    WHERE client_id = $1
                    "#,
                    stored.client_id,
                )
                .execute(&mut transaction.executor())
                .await
                .map_err(storage_error)?;

                insert_tokens(&mut transaction, grant_id, tokens, grant_expires_at).await?;
                CodeRedemption::Redeemed
            }
            CodeDecision::Reject(reason) => {
                sqlx::query!(
                    r#"
                    UPDATE oauth_authorization_codes
                    SET consumed_at = now()
                    WHERE code_hash = $1 AND consumed_at IS NULL
                    "#,
                    code.as_bytes().as_slice(),
                )
                .execute(&mut transaction.executor())
                .await
                .map_err(storage_error)?;
                CodeRedemption::Rejected(reason)
            }
            CodeDecision::RevokeGrant(reason) => {
                if let Some(grant_id) = row.grant_id {
                    revoke_grant_by_id(&mut transaction, grant_id).await?;
                }
                CodeRedemption::Rejected(reason)
            }
        };

        transaction.commit().await.map_err(storage_error)?;

        Ok(redemption)
    }

    async fn rotate_refresh_token(
        &self,
        token: &OAuthSecretHash,
        judge: &RefreshJudge,
        tokens: &NewTokenPair,
    ) -> Result<RefreshRotation, OAuthError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Locks the token and its grant, so concurrent refreshes with one
        // token queue here and later ones see it rotated.
        let Some(row) = sqlx::query!(
            r#"
            SELECT t.expires_at, t.rotated_at, g.id AS grant_id, g.client_id, g.resource,
                   g.expires_at AS grant_expires_at,
                   g.revoked_at IS NOT NULL AS "grant_revoked!"
            FROM oauth_tokens t
            INNER JOIN oauth_grants g ON g.id = t.grant_id
            WHERE t.token_hash = $1 AND t.kind = 'refresh'
            FOR UPDATE
            "#,
            token.as_bytes().as_slice(),
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?
        else {
            return Ok(RefreshRotation::Unknown);
        };

        let stored = StoredRefreshToken {
            client_id: row.client_id,
            resource: row.resource,
            expires_at: row.expires_at,
            rotated_at: row.rotated_at,
            grant_expires_at: row.grant_expires_at,
            grant_revoked: row.grant_revoked,
        };

        let rotation = match judge(&stored) {
            RefreshDecision::Rotate => {
                if stored.rotated_at.is_some() {
                    return Err(OAuthError::Storage(
                        "a rotated refresh token cannot be rotated again".to_string(),
                    ));
                }
                sqlx::query!(
                    r#"
                    UPDATE oauth_tokens
                    SET rotated_at = now()
                    WHERE token_hash = $1
                    "#,
                    token.as_bytes().as_slice(),
                )
                .execute(&mut transaction.executor())
                .await
                .map_err(storage_error)?;

                insert_tokens(&mut transaction, row.grant_id, tokens, row.grant_expires_at).await?;

                sqlx::query!(
                    r#"
                    UPDATE oauth_grants
                    SET last_used_at = now()
                    WHERE id = $1
                    "#,
                    row.grant_id,
                )
                .execute(&mut transaction.executor())
                .await
                .map_err(storage_error)?;
                RefreshRotation::Rotated
            }
            RefreshDecision::Reject(reason) => return Ok(RefreshRotation::Rejected(reason)),
            RefreshDecision::RevokeGrant(reason) => {
                revoke_grant_by_id(&mut transaction, row.grant_id).await?;
                RefreshRotation::Rejected(reason)
            }
        };

        transaction.commit().await.map_err(storage_error)?;

        Ok(rotation)
    }

    async fn find_access_token(
        &self,
        token: &OAuthSecretHash,
    ) -> Result<Option<AccessTokenGrant>, OAuthError> {
        let row = sqlx::query!(
            r#"
            SELECT u.id, u.email, u.roles, g.resource
            FROM oauth_tokens t
            INNER JOIN oauth_grants g ON g.id = t.grant_id
            INNER JOIN users u ON u.id = g.user_id
            WHERE t.token_hash = $1
              AND t.kind = 'access'
              AND t.expires_at > now()
              AND g.revoked_at IS NULL
              AND g.expires_at > now()
            "#,
            token.as_bytes().as_slice(),
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(row.map(|row| AccessTokenGrant {
            principal: UserPrincipal {
                id: UserId::from(row.id),
                email: row.email,
                roles: row.roles.into_iter().map(Role::from).collect(),
            },
            resource: row.resource,
        }))
    }

    async fn revoke_grant_of_token(
        &self,
        token: &OAuthSecretHash,
        client_id: Option<&str>,
    ) -> Result<bool, OAuthError> {
        let result = sqlx::query!(
            r#"
            UPDATE oauth_grants g
            SET revoked_at = now()
            FROM oauth_tokens t
            WHERE t.token_hash = $1
              AND g.id = t.grant_id
              AND g.revoked_at IS NULL
              AND ($2::TEXT IS NULL OR g.client_id = $2)
            "#,
            token.as_bytes().as_slice(),
            client_id,
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_active_grants(&self, user_id: &UserId) -> Result<Vec<OAuthGrant>, OAuthError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, client_id, client_name, redirect_uri, created_at, last_used_at, expires_at
            FROM oauth_grants g
            WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()
              -- A grant whose refresh token idled out can never be used again.
              AND EXISTS (
                  SELECT 1 FROM oauth_tokens t
                  WHERE t.grant_id = g.id AND t.kind = 'refresh'
                    AND t.rotated_at IS NULL AND t.expires_at > now()
              )
            ORDER BY created_at DESC, id DESC
            "#,
            user_id.as_i32(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|row| OAuthGrant {
                id: OAuthGrantId::new(row.id),
                client_id: row.client_id,
                client_name: row.client_name,
                redirect_uri: row.redirect_uri,
                created_at: row.created_at,
                last_used_at: row.last_used_at,
                expires_at: row.expires_at,
            })
            .collect())
    }

    async fn revoke_grant(
        &self,
        user_id: &UserId,
        grant_id: &OAuthGrantId,
    ) -> Result<(), OAuthError> {
        let result = sqlx::query!(
            r#"
            UPDATE oauth_grants
            SET revoked_at = now()
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > now()
            "#,
            grant_id.as_i32(),
            user_id.as_i32(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        if result.rows_affected() == 0 {
            return Err(OAuthError::NotFound);
        }

        Ok(())
    }

    async fn delete_expired(&self) -> Result<(), OAuthError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        sqlx::query!(
            r#"
            DELETE FROM oauth_authorization_requests
            WHERE expires_at <= now()
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        sqlx::query!(
            r#"
            DELETE FROM oauth_authorization_codes
            WHERE expires_at < now() - INTERVAL '1 day'
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        sqlx::query!(
            r#"
            DELETE FROM oauth_tokens
            WHERE expires_at < now() - INTERVAL '1 day' AND rotated_at IS NULL
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // Rotated refresh tokens were kept above, as their reuse revokes the
        // grant even after they expired; they go with their grant. Deleting
        // them before the grant takes token locks before grant locks, as a
        // refresh does, so cleanup and a refresh cannot deadlock.
        sqlx::query!(
            r#"
            DELETE FROM oauth_tokens
            WHERE grant_id IN (
                SELECT id FROM oauth_grants
                WHERE revoked_at < now() - INTERVAL '30 days'
                   OR expires_at < now() - INTERVAL '30 days'
            )
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // Deleting a grant deletes its remaining codes with it.
        sqlx::query!(
            r#"
            DELETE FROM oauth_grants
            WHERE revoked_at < now() - INTERVAL '30 days'
               OR expires_at < now() - INTERVAL '30 days'
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // A registration that produced a grant is kept: its client caches the
        // client_id beyond the grant's tokens. Unused ones are kept a while,
        // for clients that come back to an abandoned sign-in.
        sqlx::query!(
            r#"
            DELETE FROM oauth_clients
            WHERE last_used_at IS NULL AND created_at < now() - INTERVAL '30 days'
            "#,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;

        Ok(())
    }
}

/// Stores a grant's new access and refresh token, the refresh token expiring
/// no later than the grant.
async fn insert_tokens(
    transaction: &mut DbTransaction,
    grant_id: i32,
    tokens: &NewTokenPair,
    grant_expires_at: OffsetDateTime,
) -> Result<(), OAuthError> {
    sqlx::query!(
        r#"
        INSERT INTO oauth_tokens (token_hash, grant_id, kind, expires_at)
        VALUES ($1, $3, 'access', $4),
               ($2, $3, 'refresh', LEAST($5::TIMESTAMPTZ, $6::TIMESTAMPTZ))
        "#,
        tokens.access.hash.as_bytes().as_slice(),
        tokens.refresh.hash.as_bytes().as_slice(),
        grant_id,
        tokens.access.expires_at,
        tokens.refresh.expires_at,
        grant_expires_at,
    )
    .execute(&mut transaction.executor())
    .await
    .map_err(storage_error)?;

    Ok(())
}

async fn revoke_grant_by_id(
    transaction: &mut DbTransaction,
    grant_id: i32,
) -> Result<(), OAuthError> {
    sqlx::query!(
        r#"
        UPDATE oauth_grants
        SET revoked_at = now()
        WHERE id = $1 AND revoked_at IS NULL
        "#,
        grant_id,
    )
    .execute(&mut transaction.executor())
    .await
    .map_err(storage_error)?;

    Ok(())
}

fn registered_client(
    id: String,
    name: String,
    redirect_uris: &[String],
) -> Result<OAuthClient, OAuthError> {
    let redirect_uris = redirect_uris
        .iter()
        .map(|uri| {
            RedirectUri::parse(uri).ok_or_else(|| {
                OAuthError::Storage("stored OAuth redirect URI is invalid".to_string())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OAuthClient {
        id,
        name,
        kind: OAuthClientKind::Registered,
        redirect_uris,
    })
}

fn stored_challenge(challenge: &str) -> Result<PkceChallenge, OAuthError> {
    PkceChallenge::parse_s256(challenge)
        .ok_or_else(|| OAuthError::Storage("stored PKCE code challenge is invalid".to_string()))
}

fn storage_error(error: sqlx::Error) -> OAuthError {
    OAuthError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{Arc, Mutex},
    };

    use sqlx::PgPool;
    use time::Duration;

    use super::*;
    use crate::domain::models::{
        NewToken, OAuthSecret, OAuthSecretKind, ACCESS_TOKEN_LIFETIME, GRANT_LIFETIME,
        REFRESH_TOKEN_IDLE_LIFETIME,
    };

    const CLIENT_ID: &str = "https://claude.ai/oauth/claude-code-client-metadata";
    const OTHER_CLIENT_ID: &str = "https://chatgpt.com/oauth/client.json";
    const REDIRECT_URI: &str = "http://localhost:53682/callback";
    const RESOURCE: &str = "https://toki.example/mcp";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn repository(pool: &PgPool) -> Arc<PostgresOAuthRepository> {
        Arc::new(PostgresOAuthRepository::new(DbPool::from(pool.clone())))
    }

    async fn seed_user(pool: &PgPool, email: &str) -> UserId {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ($1, 'Test User', '', '')
             RETURNING id",
        )
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::new(id)
    }

    fn from_now(duration: Duration) -> OffsetDateTime {
        OffsetDateTime::now_utc() + duration
    }

    async fn pending_request(
        repository: &PostgresOAuthRepository,
        user_id: UserId,
        client_id: &str,
        expires_at: OffsetDateTime,
    ) -> OAuthSecretHash {
        let id = OAuthSecret::generate(OAuthSecretKind::AuthorizationRequest).hash();
        repository
            .insert_authorization_request(&NewAuthorizationRequest {
                id,
                user_id,
                client_id: client_id.to_string(),
                client_name: "Claude Code".to_string(),
                redirect_uri: REDIRECT_URI.to_string(),
                resource: RESOURCE.to_string(),
                code_challenge: PkceChallenge::parse_s256(CHALLENGE).unwrap(),
                state: Some("client-state".to_string()),
                expires_at,
            })
            .await
            .unwrap();
        id
    }

    fn new_code() -> NewAuthorizationCode {
        NewAuthorizationCode {
            hash: OAuthSecret::generate(OAuthSecretKind::AuthorizationCode).hash(),
            expires_at: from_now(Duration::minutes(5)),
        }
    }

    /// An authorization code `user_id` approved for `client_id`.
    async fn approved_code(
        repository: &PostgresOAuthRepository,
        user_id: UserId,
        client_id: &str,
    ) -> OAuthSecretHash {
        let request = pending_request(
            repository,
            user_id,
            client_id,
            from_now(Duration::minutes(10)),
        )
        .await;
        let code = new_code();
        repository
            .take_authorization_request(&request, &user_id, Some(&code))
            .await
            .unwrap()
            .unwrap();
        code.hash
    }

    struct Tokens {
        pair: NewTokenPair,
        access: OAuthSecretHash,
        refresh: OAuthSecretHash,
    }

    fn tokens_expiring(access: OffsetDateTime, refresh: OffsetDateTime) -> Tokens {
        let access_hash = OAuthSecret::generate(OAuthSecretKind::AccessToken).hash();
        let refresh_hash = OAuthSecret::generate(OAuthSecretKind::RefreshToken).hash();
        Tokens {
            pair: NewTokenPair {
                access: NewToken {
                    hash: access_hash,
                    expires_at: access,
                },
                refresh: NewToken {
                    hash: refresh_hash,
                    expires_at: refresh,
                },
            },
            access: access_hash,
            refresh: refresh_hash,
        }
    }

    fn tokens() -> Tokens {
        tokens_expiring(
            from_now(ACCESS_TOKEN_LIFETIME),
            from_now(REFRESH_TOKEN_IDLE_LIFETIME),
        )
    }

    /// The service's rule for codes: redeem once, revoke on replay.
    fn redeem_once(code: &StoredAuthorizationCode) -> CodeDecision {
        if code.consumed {
            CodeDecision::RevokeGrant("replayed".to_string())
        } else {
            CodeDecision::Redeem
        }
    }

    fn rotate_once(token: &StoredRefreshToken) -> RefreshDecision {
        if token.rotated_at.is_some() {
            RefreshDecision::Reject("rotated".to_string())
        } else {
            RefreshDecision::Rotate
        }
    }

    /// A grant for `user_id` and `client_id` expiring at `grant_expires_at`,
    /// holding `tokens`.
    async fn connect_with(
        repository: &PostgresOAuthRepository,
        user_id: UserId,
        client_id: &str,
        grant_expires_at: OffsetDateTime,
        tokens: Tokens,
    ) -> Tokens {
        let code = approved_code(repository, user_id, client_id).await;
        let redemption = repository
            .redeem_authorization_code(&code, &redeem_once, grant_expires_at, &tokens.pair)
            .await
            .unwrap();
        assert!(matches!(redemption, CodeRedemption::Redeemed));
        tokens
    }

    async fn connect(
        repository: &PostgresOAuthRepository,
        user_id: UserId,
        client_id: &str,
    ) -> Tokens {
        connect_with(
            repository,
            user_id,
            client_id,
            from_now(GRANT_LIFETIME),
            tokens(),
        )
        .await
    }

    async fn is_active(repository: &PostgresOAuthRepository, access: &OAuthSecretHash) -> bool {
        repository
            .find_access_token(access)
            .await
            .unwrap()
            .is_some()
    }

    /// Runs `attempts` concurrently while a test transaction holds the row
    /// lock `lock` takes on `key`, releasing it once every attempt waits on a
    /// lock, so they contend for the row instead of running one after another.
    async fn race<F>(
        pool: &PgPool,
        lock: &'static str,
        key: &OAuthSecretHash,
        attempts: impl IntoIterator<Item = F>,
    ) -> Vec<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let mut holder = pool.begin().await.unwrap();
        sqlx::query(lock)
            .bind(key.as_bytes().as_slice())
            .execute(&mut *holder)
            .await
            .unwrap();

        let attempts = attempts.into_iter().map(tokio::spawn).collect::<Vec<_>>();
        let waiting = || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            )
            .fetch_one(pool)
            .await
            .unwrap()
        };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while waiting().await < attempts.len() as i64 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the attempts never queued on the lock"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        holder.commit().await.unwrap();

        futures::future::try_join_all(attempts).await.unwrap()
    }

    #[sqlx::test]
    async fn a_redeemed_code_grants_the_approved_access(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let code = approved_code(&repository, user_id, CLIENT_ID).await;
        let issued = tokens();
        let seen = Arc::new(Mutex::new(None));
        let judge = {
            let seen = seen.clone();
            move |code: &StoredAuthorizationCode| {
                *seen.lock().unwrap() = Some(code.clone());
                redeem_once(code)
            }
        };

        let redemption = repository
            .redeem_authorization_code(&code, &judge, from_now(GRANT_LIFETIME), &issued.pair)
            .await
            .unwrap();

        assert!(matches!(redemption, CodeRedemption::Redeemed));
        let seen = seen.lock().unwrap().clone().unwrap();
        assert_eq!(seen.user_id, user_id);
        assert_eq!(seen.client_id, CLIENT_ID);
        assert_eq!(seen.redirect_uri, REDIRECT_URI);
        assert_eq!(seen.resource, RESOURCE);
        assert_eq!(seen.code_challenge.as_str(), CHALLENGE);
        assert!(!seen.consumed);

        let grant = repository
            .find_access_token(&issued.access)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(grant.principal.id, user_id);
        assert_eq!(grant.principal.email, "user@example.com");
        assert_eq!(grant.resource, RESOURCE);
        let grants = repository.list_active_grants(&user_id).await.unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].client_id, CLIENT_ID);
        assert_eq!(grants[0].client_name, "Claude Code");
        assert_eq!(grants[0].redirect_uri, REDIRECT_URI);

        let unknown = OAuthSecret::generate(OAuthSecretKind::AuthorizationCode).hash();
        let redemption = repository
            .redeem_authorization_code(
                &unknown,
                &redeem_once,
                from_now(GRANT_LIFETIME),
                &tokens().pair,
            )
            .await
            .unwrap();
        assert!(matches!(redemption, CodeRedemption::Unknown));
    }

    #[sqlx::test]
    async fn concurrent_redemptions_of_a_code_redeem_once_and_the_replay_revokes(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let code = approved_code(&repository, user_id, CLIENT_ID).await;
        let seen = Arc::new(Mutex::new(Vec::new()));

        let attempts = (0..2).map(|_| {
            let repository = repository.clone();
            let seen = seen.clone();
            async move {
                let issued = tokens();
                let judge = move |code: &StoredAuthorizationCode| {
                    seen.lock().unwrap().push(code.consumed);
                    redeem_once(code)
                };
                let redemption = repository
                    .redeem_authorization_code(
                        &code,
                        &judge,
                        from_now(GRANT_LIFETIME),
                        &issued.pair,
                    )
                    .await
                    .unwrap();
                (redemption, issued.access)
            }
        });
        let outcomes = race(
            &pool,
            "SELECT 1 FROM oauth_authorization_codes WHERE code_hash = $1 FOR UPDATE",
            &code,
            attempts,
        )
        .await;

        let redeemed = outcomes
            .iter()
            .filter(|(redemption, _)| matches!(redemption, CodeRedemption::Redeemed))
            .map(|(_, access)| *access)
            .collect::<Vec<_>>();
        assert_eq!(redeemed.len(), 1);
        assert!(outcomes
            .iter()
            .any(|(redemption, _)| matches!(redemption, CodeRedemption::Rejected(_))));
        let mut seen = seen.lock().unwrap().clone();
        seen.sort();
        assert_eq!(seen, [false, true]);
        assert!(!is_active(&repository, &redeemed[0]).await);
        assert!(repository
            .list_active_grants(&user_id)
            .await
            .unwrap()
            .is_empty());
    }

    #[sqlx::test]
    async fn a_rejected_code_stays_consumed_without_a_grant(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let code = approved_code(&repository, user_id, CLIENT_ID).await;

        let redemption = repository
            .redeem_authorization_code(
                &code,
                &|_| CodeDecision::Reject("wrong verifier".to_string()),
                from_now(GRANT_LIFETIME),
                &tokens().pair,
            )
            .await
            .unwrap();
        assert!(
            matches!(redemption, CodeRedemption::Rejected(reason) if reason == "wrong verifier")
        );

        let consumed = Arc::new(Mutex::new(None));
        let judge = {
            let consumed = consumed.clone();
            move |code: &StoredAuthorizationCode| {
                *consumed.lock().unwrap() = Some(code.consumed);
                redeem_once(code)
            }
        };
        repository
            .redeem_authorization_code(&code, &judge, from_now(GRANT_LIFETIME), &tokens().pair)
            .await
            .unwrap();
        assert_eq!(*consumed.lock().unwrap(), Some(true));
        assert!(repository
            .list_active_grants(&user_id)
            .await
            .unwrap()
            .is_empty());
    }

    #[sqlx::test]
    async fn rotation_stores_a_usable_pair_and_marks_the_old_token_rotated(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let first = connect(&repository, user_id, CLIENT_ID).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let judge = {
            let seen = seen.clone();
            move |token: &StoredRefreshToken| {
                seen.lock().unwrap().push(token.clone());
                rotate_once(token)
            }
        };

        let second = tokens();
        let rotation = repository
            .rotate_refresh_token(&first.refresh, &judge, &second.pair)
            .await
            .unwrap();
        assert!(matches!(rotation, RefreshRotation::Rotated));
        assert!(is_active(&repository, &second.access).await);

        let rotation = repository
            .rotate_refresh_token(&second.refresh, &judge, &tokens().pair)
            .await
            .unwrap();
        assert!(matches!(rotation, RefreshRotation::Rotated));

        let rejected = tokens();
        let rotation = repository
            .rotate_refresh_token(&first.refresh, &judge, &rejected.pair)
            .await
            .unwrap();
        assert!(matches!(rotation, RefreshRotation::Rejected(reason) if reason == "rotated"));
        assert!(!is_active(&repository, &rejected.access).await);

        let seen = seen.lock().unwrap().clone();
        assert_eq!(
            seen.iter()
                .map(|token| token.rotated_at.is_some())
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        assert!(seen.iter().all(|token| token.client_id == CLIENT_ID
            && token.resource == RESOURCE
            && !token.grant_revoked));
    }

    #[sqlx::test]
    async fn a_refresh_token_reused_late_revokes_its_grant(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let first = connect(&repository, user_id, CLIENT_ID).await;
        let second = tokens();
        repository
            .rotate_refresh_token(&first.refresh, &rotate_once, &second.pair)
            .await
            .unwrap();

        let rotation = repository
            .rotate_refresh_token(
                &first.refresh,
                &|_| RefreshDecision::RevokeGrant("reused".to_string()),
                &tokens().pair,
            )
            .await
            .unwrap();

        assert!(matches!(rotation, RefreshRotation::Rejected(_)));
        assert!(!is_active(&repository, &second.access).await);
        let revoked = Arc::new(Mutex::new(None));
        let judge = {
            let revoked = revoked.clone();
            move |token: &StoredRefreshToken| {
                *revoked.lock().unwrap() = Some(token.grant_revoked);
                RefreshDecision::Reject("revoked".to_string())
            }
        };
        repository
            .rotate_refresh_token(&second.refresh, &judge, &tokens().pair)
            .await
            .unwrap();
        assert_eq!(*revoked.lock().unwrap(), Some(true));
    }

    #[sqlx::test]
    async fn an_access_token_is_unknown_to_rotation(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let issued = connect(&repository, user_id, CLIENT_ID).await;

        let rotation = repository
            .rotate_refresh_token(
                &issued.access,
                &|_| unreachable!("an access token is not a refresh token"),
                &tokens().pair,
            )
            .await
            .unwrap();

        assert!(matches!(rotation, RefreshRotation::Unknown));
    }

    async fn refresh_expires_with_grant(pool: &PgPool, refresh: &OAuthSecretHash) -> bool {
        sqlx::query_scalar(
            "SELECT t.expires_at = g.expires_at
             FROM oauth_tokens t
             INNER JOIN oauth_grants g ON g.id = t.grant_id
             WHERE t.token_hash = $1",
        )
        .bind(refresh.as_bytes().as_slice())
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn refresh_tokens_expire_no_later_than_their_grant(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;

        let lasting = connect(&repository, user_id, CLIENT_ID).await;
        assert!(!refresh_expires_with_grant(&pool, &lasting.refresh).await);

        let ending = connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(Duration::days(1)),
            tokens(),
        )
        .await;
        assert!(refresh_expires_with_grant(&pool, &ending.refresh).await);

        let rotated = tokens();
        repository
            .rotate_refresh_token(&ending.refresh, &rotate_once, &rotated.pair)
            .await
            .unwrap();
        assert!(refresh_expires_with_grant(&pool, &rotated.refresh).await);
    }

    #[sqlx::test]
    async fn concurrent_rotations_of_a_refresh_token_rotate_once(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let first = connect(&repository, user_id, CLIENT_ID).await;
        let refresh = first.refresh;
        let seen = Arc::new(Mutex::new(Vec::new()));

        let attempts = (0..2).map(|_| {
            let repository = repository.clone();
            let seen = seen.clone();
            async move {
                let issued = tokens();
                let judge = move |token: &StoredRefreshToken| {
                    seen.lock().unwrap().push(token.rotated_at.is_some());
                    rotate_once(token)
                };
                let rotation = repository
                    .rotate_refresh_token(&refresh, &judge, &issued.pair)
                    .await
                    .unwrap();
                (rotation, issued.access)
            }
        });
        let outcomes = race(
            &pool,
            "SELECT 1 FROM oauth_tokens WHERE token_hash = $1 FOR UPDATE",
            &refresh,
            attempts,
        )
        .await;

        let rotated = outcomes
            .iter()
            .filter(|(rotation, _)| matches!(rotation, RefreshRotation::Rotated))
            .map(|(_, access)| *access)
            .collect::<Vec<_>>();
        assert_eq!(rotated.len(), 1);
        let mut seen = seen.lock().unwrap().clone();
        seen.sort();
        assert_eq!(seen, [false, true]);
        assert!(is_active(&repository, &rotated[0]).await);
    }

    async fn count(pool: &PgPool, table: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn racing_approval_and_denial_decide_a_request_once(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let request = pending_request(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(Duration::minutes(10)),
        )
        .await;

        let decisions = [Some(new_code()), None].map(|code| {
            let repository = repository.clone();
            async move {
                let decided = repository
                    .take_authorization_request(&request, &user_id, code.as_ref())
                    .await
                    .unwrap();
                (decided.is_some(), code.is_some())
            }
        });
        let outcomes = race(
            &pool,
            "SELECT 1 FROM oauth_authorization_requests WHERE id_hash = $1 FOR UPDATE",
            &request,
            decisions,
        )
        .await;

        let winners = outcomes
            .iter()
            .filter(|(decided, _)| *decided)
            .collect::<Vec<_>>();
        assert_eq!(winners.len(), 1);
        let approved = winners[0].1;
        assert_eq!(
            count(&pool, "oauth_authorization_codes").await,
            i64::from(approved)
        );
    }

    #[sqlx::test]
    async fn only_the_requests_user_can_decide_it(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let other_id = seed_user(&pool, "other@example.com").await;
        let request = pending_request(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(Duration::minutes(10)),
        )
        .await;

        let taken = repository
            .take_authorization_request(&request, &other_id, Some(&new_code()))
            .await
            .unwrap();
        assert!(taken.is_none());
        assert_eq!(count(&pool, "oauth_authorization_codes").await, 0);

        let decided = repository
            .take_authorization_request(&request, &user_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(decided.redirect_uri, REDIRECT_URI);
        assert_eq!(decided.state.as_deref(), Some("client-state"));
    }

    #[sqlx::test]
    async fn an_expired_request_cannot_be_decided(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let request = pending_request(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(-Duration::seconds(1)),
        )
        .await;

        let taken = repository
            .take_authorization_request(&request, &user_id, Some(&new_code()))
            .await
            .unwrap();

        assert!(taken.is_none());
        assert_eq!(count(&pool, "oauth_authorization_codes").await, 0);
    }

    #[sqlx::test]
    async fn access_tokens_stop_working_with_their_grant_or_expiry(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;

        let active = connect(&repository, user_id, CLIENT_ID).await;
        assert!(is_active(&repository, &active.access).await);
        assert!(!is_active(&repository, &active.refresh).await);

        let revoked = connect(&repository, user_id, OTHER_CLIENT_ID).await;
        let grant_id = repository.list_active_grants(&user_id).await.unwrap()[0].id;
        repository.revoke_grant(&user_id, &grant_id).await.unwrap();
        assert!(!is_active(&repository, &revoked.access).await);

        let expired_grant = connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(-Duration::minutes(1)),
            tokens(),
        )
        .await;
        assert!(!is_active(&repository, &expired_grant.access).await);

        let expired_token = connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(GRANT_LIFETIME),
            tokens_expiring(
                from_now(-Duration::minutes(1)),
                from_now(REFRESH_TOKEN_IDLE_LIFETIME),
            ),
        )
        .await;
        assert!(!is_active(&repository, &expired_token.access).await);

        assert!(is_active(&repository, &active.access).await);
    }

    #[sqlx::test]
    async fn revoking_by_token_respects_the_presenting_client(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let issued = connect(&repository, user_id, CLIENT_ID).await;

        let revoked = repository
            .revoke_grant_of_token(&issued.refresh, Some(OTHER_CLIENT_ID))
            .await
            .unwrap();
        assert!(!revoked);
        assert!(is_active(&repository, &issued.access).await);

        let revoked = repository
            .revoke_grant_of_token(&issued.refresh, Some(CLIENT_ID))
            .await
            .unwrap();
        assert!(revoked);
        assert!(!is_active(&repository, &issued.access).await);
        assert!(!repository
            .revoke_grant_of_token(&issued.access, None)
            .await
            .unwrap());

        let other = connect(&repository, user_id, CLIENT_ID).await;
        assert!(repository
            .revoke_grant_of_token(&other.access, None)
            .await
            .unwrap());
        assert!(!is_active(&repository, &other.access).await);

        let unknown = OAuthSecret::generate(OAuthSecretKind::AccessToken).hash();
        assert!(!repository
            .revoke_grant_of_token(&unknown, None)
            .await
            .unwrap());
    }

    #[sqlx::test]
    async fn users_list_and_revoke_only_their_active_grants(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        let other_id = seed_user(&pool, "other@example.com").await;
        connect(&repository, user_id, CLIENT_ID).await;
        connect(&repository, user_id, OTHER_CLIENT_ID).await;
        connect(&repository, other_id, CLIENT_ID).await;
        connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(-Duration::minutes(1)),
            tokens(),
        )
        .await;
        // Its refresh token idled out, so it can never be used again.
        connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(GRANT_LIFETIME),
            tokens_expiring(
                from_now(ACCESS_TOKEN_LIFETIME),
                from_now(-Duration::minutes(1)),
            ),
        )
        .await;

        let grants = repository.list_active_grants(&user_id).await.unwrap();
        assert_eq!(
            grants
                .iter()
                .map(|grant| grant.client_id.as_str())
                .collect::<Vec<_>>(),
            [OTHER_CLIENT_ID, CLIENT_ID]
        );

        let newest = grants[0].id;
        assert!(matches!(
            repository.revoke_grant(&other_id, &newest).await,
            Err(OAuthError::NotFound)
        ));
        repository.revoke_grant(&user_id, &newest).await.unwrap();
        assert!(matches!(
            repository.revoke_grant(&user_id, &newest).await,
            Err(OAuthError::NotFound)
        ));

        let grants = repository.list_active_grants(&user_id).await.unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].client_id, CLIENT_ID);
        assert_eq!(
            repository
                .list_active_grants(&other_id)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    fn registration(id: &str) -> NewRegisteredClient {
        NewRegisteredClient {
            id: id.to_string(),
            name: "Cursor".to_string(),
            redirect_uris: [
                "cursor://anysphere.cursor-mcp/oauth/callback",
                "http://127.0.0.1/callback",
            ]
            .map(|uri| RedirectUri::parse(uri).unwrap())
            .to_vec(),
        }
    }

    #[sqlx::test]
    async fn registered_clients_are_stored_and_found(pool: PgPool) {
        let repository = repository(&pool);
        let registration = registration("0123456789abcdef0123456789abcdef");

        let stored = repository
            .insert_registered_client(&registration)
            .await
            .unwrap();
        let expected = OAuthClient {
            id: registration.id.clone(),
            name: registration.name.clone(),
            kind: OAuthClientKind::Registered,
            redirect_uris: registration.redirect_uris.clone(),
        };
        assert_eq!(stored.client, expected);
        assert_eq!(
            repository
                .find_registered_client(&registration.id)
                .await
                .unwrap(),
            Some(expected)
        );
        assert_eq!(
            repository.find_registered_client("unknown").await.unwrap(),
            None
        );

        sqlx::query("UPDATE oauth_clients SET redirect_uris = ARRAY['javascript:alert(1)']")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            repository.find_registered_client(&registration.id).await,
            Err(OAuthError::Storage(_))
        ));
    }

    #[sqlx::test]
    async fn cleanup_removes_expired_records_and_unused_registrations(pool: PgPool) {
        let repository = repository(&pool);
        let user_id = seed_user(&pool, "user@example.com").await;
        for id in ["used", "unused-old", "unused-new"] {
            repository
                .insert_registered_client(&registration(id))
                .await
                .unwrap();
        }
        connect(&repository, user_id, "used").await;
        sqlx::query(
            "UPDATE oauth_clients SET created_at = now() - INTERVAL '31 days'
             WHERE client_id IN ('used', 'unused-old')",
        )
        .execute(&pool)
        .await
        .unwrap();

        pending_request(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(-Duration::seconds(1)),
        )
        .await;
        pending_request(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(Duration::minutes(10)),
        )
        .await;

        let recently_revoked = connect(&repository, user_id, CLIENT_ID).await;
        repository
            .revoke_grant_of_token(&recently_revoked.access, None)
            .await
            .unwrap();
        let long_revoked = connect(&repository, user_id, CLIENT_ID).await;
        repository
            .revoke_grant_of_token(&long_revoked.access, None)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE oauth_grants SET revoked_at = now() - INTERVAL '31 days'
             WHERE id = (SELECT grant_id FROM oauth_tokens WHERE token_hash = $1)",
        )
        .bind(long_revoked.access.as_bytes().as_slice())
        .execute(&pool)
        .await
        .unwrap();
        let long_expired_access = connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(GRANT_LIFETIME),
            tokens_expiring(
                from_now(-Duration::days(2)),
                from_now(REFRESH_TOKEN_IDLE_LIFETIME),
            ),
        )
        .await;
        let expired_refresh = tokens_expiring(
            from_now(ACCESS_TOKEN_LIFETIME),
            from_now(-Duration::days(2)),
        );
        connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(GRANT_LIFETIME),
            expired_refresh,
        )
        .await;
        let rotated_expired = connect_with(
            &repository,
            user_id,
            CLIENT_ID,
            from_now(GRANT_LIFETIME),
            tokens_expiring(
                from_now(ACCESS_TOKEN_LIFETIME),
                from_now(-Duration::days(2)),
            ),
        )
        .await;
        sqlx::query(
            "UPDATE oauth_tokens SET rotated_at = now() - INTERVAL '3 days' WHERE token_hash = $1",
        )
        .bind(rotated_expired.refresh.as_bytes().as_slice())
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(count(&pool, "oauth_grants").await, 6);
        assert_eq!(count(&pool, "oauth_tokens").await, 12);
        assert_eq!(count(&pool, "oauth_authorization_codes").await, 6);

        repository.delete_expired().await.unwrap();

        for (id, kept) in [("used", true), ("unused-old", false), ("unused-new", true)] {
            let found = repository.find_registered_client(id).await.unwrap();
            assert_eq!(found.is_some(), kept, "{id}");
        }
        assert_eq!(count(&pool, "oauth_authorization_requests").await, 1);
        assert_eq!(count(&pool, "oauth_grants").await, 5);
        // The long-revoked grant took its code and both its tokens with it;
        // the long-expired access token and the expired, never-rotated refresh
        // token went alone; the expired rotated refresh token stays.
        assert_eq!(count(&pool, "oauth_authorization_codes").await, 5);
        assert_eq!(count(&pool, "oauth_tokens").await, 8);
        assert!(repository
            .revoke_grant_of_token(&long_expired_access.refresh, None)
            .await
            .unwrap());
        assert!(repository
            .revoke_grant_of_token(&rotated_expired.refresh, None)
            .await
            .unwrap());
    }
}
