use async_trait::async_trait;

use crate::domain::{
    models::{ApiToken, ApiTokenGrant, ApiTokenHash, ApiTokenId, NewApiToken, UserId},
    ApiTokenError,
};

#[async_trait]
pub trait ApiTokenRepository: Send + Sync + 'static {
    async fn insert_if_below_limit(
        &self,
        user_id: &UserId,
        token: &NewApiToken,
        max_tokens: usize,
    ) -> Result<ApiToken, ApiTokenError>;

    async fn list_for_user(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError>;

    async fn delete_for_user(
        &self,
        user_id: &UserId,
        token_id: &ApiTokenId,
    ) -> Result<(), ApiTokenError>;

    async fn find_grant_by_token_hash(
        &self,
        hash: &ApiTokenHash,
    ) -> Result<Option<ApiTokenGrant>, ApiTokenError>;
}
