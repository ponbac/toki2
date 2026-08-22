use async_trait::async_trait;

use crate::domain::{
    models::{ApiToken, ApiTokenCapabilities, ApiTokenGrant, ApiTokenId, IssuedApiToken, UserId},
    ApiTokenError,
};

#[async_trait]
pub trait ApiTokenService: Send + Sync + 'static {
    async fn create(
        &self,
        user_id: &UserId,
        name: &str,
        capabilities: ApiTokenCapabilities,
    ) -> Result<IssuedApiToken, ApiTokenError>;

    async fn list(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError>;

    async fn revoke(&self, user_id: &UserId, token_id: &ApiTokenId) -> Result<(), ApiTokenError>;
}

/// Resolves API-token credentials without exposing provider or session secrets.
#[async_trait]
pub trait ApiTokenAuthenticator: Send + Sync + 'static {
    async fn authenticate(&self, presented: &str) -> Result<Option<ApiTokenGrant>, ApiTokenError>;
}
