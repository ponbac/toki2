use async_trait::async_trait;

use crate::domain::{
    models::{
        AuthorizationParams, AuthorizationResponse, ClientRegistration, CodeExchange, IssuedTokens,
        OAuthGrant, OAuthGrantId, OAuthSecret, RegisteredClient, TokenRefresh, UserId,
        ValidatedAuthorization,
    },
    AuthorizationError, OAuthError, UserPrincipal,
};

/// Use cases of the OAuth authorization server that MCP clients sign in
/// through.
#[async_trait]
pub trait OAuthService: Send + Sync + 'static {
    /// Validates an authorization request before the user is asked about it.
    async fn validate_authorization(
        &self,
        params: AuthorizationParams,
    ) -> Result<ValidatedAuthorization, AuthorizationError>;

    /// Records a validated request shown to `user_id` for consent, and
    /// returns the handle the consent form posts back.
    async fn begin_consent(
        &self,
        user_id: &UserId,
        authorization: &ValidatedAuthorization,
    ) -> Result<OAuthSecret, OAuthError>;

    /// Applies `user_id`'s decision on a consent request, once. `NotFound`
    /// when the request is unknown, expired, already decided, or another
    /// user's.
    async fn decide(
        &self,
        user_id: &UserId,
        request: &str,
        allow: bool,
    ) -> Result<AuthorizationResponse, OAuthError>;

    async fn exchange_code(&self, exchange: CodeExchange) -> Result<IssuedTokens, OAuthError>;

    async fn refresh(&self, refresh: TokenRefresh) -> Result<IssuedTokens, OAuthError>;

    /// Revokes the connection `token`, an access or refresh token, belongs
    /// to. The client must name itself (`InvalidClient` otherwise); unknown
    /// tokens and another client's tokens are ignored (RFC 7009).
    async fn revoke_token(&self, token: &str, client_id: Option<&str>) -> Result<(), OAuthError>;

    async fn register_client(
        &self,
        registration: ClientRegistration,
    ) -> Result<RegisteredClient, OAuthError>;

    /// The user's active connections.
    async fn list_grants(&self, user_id: &UserId) -> Result<Vec<OAuthGrant>, OAuthError>;

    async fn revoke_grant(
        &self,
        user_id: &UserId,
        grant_id: &OAuthGrantId,
    ) -> Result<(), OAuthError>;

    /// Deletes expired and abandoned records.
    async fn remove_expired(&self) -> Result<(), OAuthError>;
}

/// Resolves the access tokens `/mcp` accepts.
#[async_trait]
pub trait OAuthAccessTokenAuthenticator: Send + Sync + 'static {
    async fn authenticate(&self, presented: &str) -> Result<Option<UserPrincipal>, OAuthError>;
}
