use async_trait::async_trait;
use axum_login::{AuthnBackend, UserId};
use oauth2::{
    basic::{BasicClient, BasicRequestTokenError},
    reqwest::{async_http_client, AsyncHttpClientError},
    AuthorizationCode, CsrfToken, TokenResponse,
};
use reqwest::{
    header::{AUTHORIZATION, USER_AGENT},
    Url,
};
use serde::Deserialize;
use sqlx::PgPool;

use super::User;

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub code: String,
    pub old_state: CsrfToken,
    pub new_state: CsrfToken,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(rename = "name")]
    full_name: String,
    picture: String,
    email: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error(transparent)]
    Sqlx(sqlx::Error),

    #[error(transparent)]
    Reqwest(reqwest::Error),

    #[error(transparent)]
    OAuth2(BasicRequestTokenError<AsyncHttpClientError>),
}

#[derive(Debug, Clone)]
pub struct AuthBackend {
    db: PgPool,
    client: BasicClient,
}

impl AuthBackend {
    pub fn new(db: PgPool, client: BasicClient) -> Self {
        Self { db, client }
    }

    pub fn authorize_url(&self) -> (Url, CsrfToken) {
        self.client.authorize_url(CsrfToken::new_random).url()
    }
}

#[async_trait]
impl AuthnBackend for AuthBackend {
    type User = User;
    type Credentials = Credentials;
    type Error = BackendError;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        // Ensure the CSRF state has not been tampered with.
        if creds.old_state.secret() != creds.new_state.secret() {
            return Ok(None);
        };

        // Process authorization code, expecting a token response back.
        let token_res = self
            .client
            .exchange_code(AuthorizationCode::new(creds.code))
            .request_async(async_http_client)
            .await
            .map_err(Self::Error::OAuth2)?;

        // Use access token to request user info.
        let user_info = reqwest::Client::new()
            .get("https://graph.microsoft.com/oidc/userinfo")
            .header(USER_AGENT.as_str(), "toki-login")
            .header(
                AUTHORIZATION.as_str(),
                format!("Bearer {}", token_res.access_token().secret()),
            )
            .send()
            .await
            .map_err(Self::Error::Reqwest)?
            .json::<UserInfo>()
            .await
            .map_err(Self::Error::Reqwest)?;

        // Persist user in our database so we can use `get_user`.
        let user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (email, full_name, picture, access_token)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(email) DO UPDATE
            SET full_name = EXCLUDED.full_name,
                picture = EXCLUDED.picture,
                access_token = EXCLUDED.access_token
            RETURNING *
            "#,
            user_info.email,
            user_info.full_name,
            user_info.picture,
            token_res.access_token().secret()
        )
        .fetch_one(&self.db)
        .await
        .map_err(Self::Error::Sqlx)?;

        Ok(Some(user))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(sqlx::query_as!(
            User,
            r#"
            SELECT * FROM users WHERE id = $1
            "#,
            *user_id as i32
        )
        .fetch_optional(&self.db)
        .await
        .map_err(Self::Error::Sqlx)?)
    }
}

pub type AuthSession = axum_login::AuthSession<AuthBackend>;
