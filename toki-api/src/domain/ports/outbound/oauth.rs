use std::sync::Arc;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AccessTokenGrant, ClientIdUrl, ClientMetadataDocument, CodeDecision, CodeRedemption,
        DecidedAuthorizationRequest, NewAuthorizationCode, NewAuthorizationRequest,
        NewRegisteredClient, NewTokenPair, OAuthClient, OAuthGrant, OAuthGrantId, OAuthSecretHash,
        RefreshDecision, RefreshRotation, RegisteredClient, StoredAuthorizationCode,
        StoredRefreshToken, UserId,
    },
    OAuthError,
};

/// Judges a locked authorization code within its redemption transaction.
pub type CodeJudge = dyn Fn(&StoredAuthorizationCode) -> CodeDecision + Send + Sync;

/// Judges a locked refresh token within its rotation transaction.
pub type RefreshJudge = dyn Fn(&StoredRefreshToken) -> RefreshDecision + Send + Sync;

/// Storage for the authorization server. Operations that must not interleave
/// with each other run in one transaction and are documented as such.
#[async_trait]
pub trait OAuthRepository: Send + Sync + 'static {
    async fn find_registered_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuthClient>, OAuthError>;

    async fn insert_registered_client(
        &self,
        client: &NewRegisteredClient,
    ) -> Result<RegisteredClient, OAuthError>;

    async fn insert_authorization_request(
        &self,
        request: &NewAuthorizationRequest,
    ) -> Result<(), OAuthError>;

    /// Atomically removes `user_id`'s unexpired request `id`, and with `code`
    /// stores that authorization code for it in the same transaction. `None`
    /// when there is no such request, so a request is decided at most once.
    async fn take_authorization_request(
        &self,
        id: &OAuthSecretHash,
        user_id: &UserId,
        code: Option<&NewAuthorizationCode>,
    ) -> Result<Option<DecidedAuthorizationRequest>, OAuthError>;

    /// Redeems an authorization code in one transaction: locks its row, asks
    /// `judge`, and applies the decision before committing.
    /// - `Redeem`: marks the code consumed, creates its grant expiring at
    ///   `grant_expires_at` and links the code to it, marks a registered
    ///   client used, and stores `tokens`.
    /// - `Reject`: marks the code consumed.
    /// - `RevokeGrant`: revokes the grant the code produced, if any.
    async fn redeem_authorization_code(
        &self,
        code: &OAuthSecretHash,
        judge: &CodeJudge,
        grant_expires_at: OffsetDateTime,
        tokens: &NewTokenPair,
    ) -> Result<CodeRedemption, OAuthError>;

    /// Rotates a refresh token in one transaction: locks it and its grant,
    /// asks `judge`, and applies the decision before committing.
    /// - `Rotate`: marks the token rotated unless it already was, stores
    ///   `tokens` (the refresh token's expiry capped at the grant's) and
    ///   records the grant's use.
    /// - `Reject`: changes nothing.
    ///
    /// Only refresh tokens are found: an access token is `Unknown` here.
    async fn rotate_refresh_token(
        &self,
        token: &OAuthSecretHash,
        judge: &RefreshJudge,
        tokens: &NewTokenPair,
    ) -> Result<RefreshRotation, OAuthError>;

    /// The user and resource of an unexpired access token whose grant is
    /// neither revoked nor expired.
    async fn find_access_token(
        &self,
        token: &OAuthSecretHash,
    ) -> Result<Option<AccessTokenGrant>, OAuthError>;

    /// Revokes the grant `token`, of either kind, belongs to, unless
    /// `client_id` is given and is not the grant's. Whether a grant was
    /// revoked.
    async fn revoke_grant_of_token(
        &self,
        token: &OAuthSecretHash,
        client_id: Option<&str>,
    ) -> Result<bool, OAuthError>;

    /// The user's grants that are neither revoked nor expired, newest first.
    async fn list_active_grants(&self, user_id: &UserId) -> Result<Vec<OAuthGrant>, OAuthError>;

    /// `NotFound` when the user has no such active grant.
    async fn revoke_grant(
        &self,
        user_id: &UserId,
        grant_id: &OAuthGrantId,
    ) -> Result<(), OAuthError>;

    /// Deletes expired consent requests; codes and tokens, used or not,
    /// expired over a day ago; grants revoked or expired over 30 days ago, with
    /// their tokens; and registered clients that never produced a grant and are
    /// over 30 days old.
    async fn delete_expired(&self) -> Result<(), OAuthError>;
}

/// Fetches Client ID Metadata Documents.
#[async_trait]
pub trait ClientMetadataResolver: Send + Sync + 'static {
    async fn resolve(
        &self,
        client_id: &ClientIdUrl,
    ) -> Result<Arc<ClientMetadataDocument>, OAuthError>;
}
