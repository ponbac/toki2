use async_trait::async_trait;

use crate::domain::{
    models::{ApiToken, ApiTokenId, IssuedApiToken, UserId},
    ApiTokenError, UserPrincipal,
};

#[async_trait]
pub trait ApiTokenService: Send + Sync + 'static {
    async fn create(&self, user_id: &UserId, name: &str) -> Result<IssuedApiToken, ApiTokenError>;

    async fn list(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError>;

    async fn revoke(&self, user_id: &UserId, token_id: &ApiTokenId) -> Result<(), ApiTokenError>;
}

/// Resolves API-token credentials without exposing provider or session secrets.
#[async_trait]
pub trait ApiTokenAuthenticator: Send + Sync + 'static {
    async fn authenticate(&self, presented: &str) -> Result<Option<UserPrincipal>, ApiTokenError>;
}
