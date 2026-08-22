use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{
        ApiToken, ApiTokenCapabilities, ApiTokenGrant, ApiTokenId, ApiTokenName, ApiTokenSecret,
        IssuedApiToken, NewApiToken, UserId, MAX_TOKENS_PER_USER,
    },
    ports::{
        inbound::{ApiTokenAuthenticator, ApiTokenService},
        outbound::ApiTokenRepository,
    },
    ApiTokenError,
};

pub struct ApiTokenServiceImpl<R> {
    repository: Arc<R>,
}

impl<R> ApiTokenServiceImpl<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R: ApiTokenRepository> ApiTokenService for ApiTokenServiceImpl<R> {
    async fn create(
        &self,
        user_id: &UserId,
        name: &str,
        capabilities: ApiTokenCapabilities,
    ) -> Result<IssuedApiToken, ApiTokenError> {
        let name = ApiTokenName::parse(name).ok_or(ApiTokenError::InvalidName)?;
        let secret = ApiTokenSecret::generate();
        let new_token = NewApiToken::new(name, &secret, capabilities);
        let token = self
            .repository
            .insert_if_below_limit(user_id, &new_token, MAX_TOKENS_PER_USER)
            .await?;

        Ok(IssuedApiToken { token, secret })
    }

    async fn list(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError> {
        self.repository.list_for_user(user_id).await
    }

    async fn revoke(&self, user_id: &UserId, token_id: &ApiTokenId) -> Result<(), ApiTokenError> {
        self.repository.delete_for_user(user_id, token_id).await
    }
}

#[async_trait]
impl<R: ApiTokenRepository> ApiTokenAuthenticator for ApiTokenServiceImpl<R> {
    async fn authenticate(&self, presented: &str) -> Result<Option<ApiTokenGrant>, ApiTokenError> {
        let Some(secret) = ApiTokenSecret::parse(presented) else {
            return Ok(None);
        };

        self.repository
            .find_grant_by_token_hash(&secret.hash())
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use time::OffsetDateTime;

    use crate::domain::{
        models::{ApiTokenHash, ApiTokenId},
        Role, UserPrincipal,
    };

    use super::*;

    struct MemoryTokens {
        next_id: Mutex<i32>,
        tokens: Mutex<Vec<(ApiToken, ApiTokenHash)>>,
        users: HashMap<UserId, UserPrincipal>,
    }

    impl MemoryTokens {
        fn with_user(user: UserPrincipal) -> Self {
            let mut users = HashMap::new();
            users.insert(user.id, user);
            Self {
                next_id: Mutex::new(1),
                tokens: Mutex::new(Vec::new()),
                users,
            }
        }
    }

    #[async_trait]
    impl ApiTokenRepository for MemoryTokens {
        async fn insert_if_below_limit(
            &self,
            user_id: &UserId,
            new_token: &NewApiToken,
            max_tokens: usize,
        ) -> Result<ApiToken, ApiTokenError> {
            let mut tokens = self.tokens.lock().unwrap();
            if tokens
                .iter()
                .filter(|(token, _)| token.user_id == *user_id)
                .count()
                >= max_tokens
            {
                return Err(ApiTokenError::TooManyTokens);
            }

            let mut next_id = self.next_id.lock().unwrap();
            let id = ApiTokenId::new(*next_id);
            *next_id += 1;

            let token = ApiToken {
                id,
                user_id: *user_id,
                name: new_token.name().clone(),
                prefix: new_token.prefix().to_string(),
                capabilities: new_token.capabilities().clone(),
                created_at: OffsetDateTime::now_utc(),
            };
            tokens.push((token.clone(), *new_token.hash()));
            Ok(token)
        }

        async fn list_for_user(&self, user_id: &UserId) -> Result<Vec<ApiToken>, ApiTokenError> {
            Ok(self
                .tokens
                .lock()
                .unwrap()
                .iter()
                .filter(|(token, _)| token.user_id == *user_id)
                .map(|(token, _)| token.clone())
                .collect())
        }

        async fn delete_for_user(
            &self,
            user_id: &UserId,
            token_id: &ApiTokenId,
        ) -> Result<(), ApiTokenError> {
            let mut tokens = self.tokens.lock().unwrap();
            let index = tokens
                .iter()
                .position(|(token, _)| token.id == *token_id && token.user_id == *user_id)
                .ok_or(ApiTokenError::NotFound)?;
            tokens.remove(index);
            Ok(())
        }

        async fn find_grant_by_token_hash(
            &self,
            hash: &ApiTokenHash,
        ) -> Result<Option<ApiTokenGrant>, ApiTokenError> {
            let tokens = self.tokens.lock().unwrap();
            let Some((token, _)) = tokens.iter().find(|(_, stored)| stored == hash) else {
                return Ok(None);
            };
            Ok(self
                .users
                .get(&token.user_id)
                .cloned()
                .map(|principal| ApiTokenGrant {
                    principal,
                    capabilities: token.capabilities.clone(),
                }))
        }
    }

    fn sample_user() -> UserPrincipal {
        UserPrincipal {
            id: UserId::from(7),
            email: "ada@example.com".to_string(),
            roles: vec![Role::User],
        }
    }

    #[tokio::test]
    async fn create_authenticates_and_revoke_stops_working() {
        let user = sample_user();
        let service = ApiTokenServiceImpl::new(Arc::new(MemoryTokens::with_user(user.clone())));

        let issued = service
            .create(
                &user.id,
                "Omarchy bar",
                ApiTokenCapabilities::timer_read_only(),
            )
            .await
            .unwrap();
        assert_eq!(issued.token.name.as_str(), "Omarchy bar");
        assert!(issued.token.prefix.starts_with("toki_"));
        assert!(issued
            .token
            .capabilities
            .contains(crate::domain::models::ApiTokenCapability::TimerRead));

        let authenticated = service
            .authenticate(issued.secret.as_str())
            .await
            .unwrap()
            .expect("issued secret should authenticate");
        assert_eq!(authenticated.principal.id, user.id);
        assert!(authenticated
            .capabilities
            .contains(crate::domain::models::ApiTokenCapability::TimerRead));

        service.revoke(&user.id, &issued.token.id).await.unwrap();
        assert!(service
            .authenticate(issued.secret.as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn create_enforces_the_per_user_limit_at_the_repository_boundary() {
        let user = sample_user();
        let service = ApiTokenServiceImpl::new(Arc::new(MemoryTokens::with_user(user.clone())));

        for index in 0..MAX_TOKENS_PER_USER {
            service
                .create(
                    &user.id,
                    &format!("Token {index}"),
                    ApiTokenCapabilities::timer_read_only(),
                )
                .await
                .expect("tokens below the limit should be created");
        }

        assert!(matches!(
            service
                .create(
                    &user.id,
                    "One too many",
                    ApiTokenCapabilities::timer_read_only()
                )
                .await,
            Err(ApiTokenError::TooManyTokens)
        ));
        assert_eq!(
            service.list(&user.id).await.unwrap().len(),
            MAX_TOKENS_PER_USER
        );
    }

    #[tokio::test]
    async fn unknown_secret_does_not_authenticate() {
        let user = sample_user();
        let service = ApiTokenServiceImpl::new(Arc::new(MemoryTokens::with_user(user)));
        assert!(service.authenticate("not-a-token").await.unwrap().is_none());
        assert!(service
            .authenticate(ApiTokenSecret::generate().as_str())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn authenticate_returns_the_capabilities_persisted_on_the_token() {
        let user = sample_user();
        let service = ApiTokenServiceImpl::new(Arc::new(MemoryTokens::with_user(user.clone())));
        let capabilities = ApiTokenCapabilities::parse(["timer:read", "catalog:read"]).unwrap();

        let issued = service
            .create(&user.id, "Agent reads", capabilities.clone())
            .await
            .unwrap();
        let grant = service
            .authenticate(issued.secret.as_str())
            .await
            .unwrap()
            .expect("issued secret should authenticate");

        assert_eq!(grant.capabilities, issued.token.capabilities);
        assert!(grant
            .capabilities
            .contains(crate::domain::models::ApiTokenCapability::CatalogRead));
    }
}
