use std::collections::HashSet;

use axum_login::{AuthnBackend, AuthzBackend, UserId as SessionUserId};
use oauth2::{
    basic::{BasicClient, BasicRequestTokenError},
    AuthorizationCode, CsrfToken, EndpointNotSet, EndpointSet, HttpClientError, HttpRequest,
    HttpResponse, TokenResponse,
};
use reqwest::{
    header::{AUTHORIZATION, USER_AGENT},
    Url,
};
use serde::Deserialize;
use tracing::Instrument;

use crate::{
    db::DbPool,
    domain::{models::UserId, Role, User},
    repositories::{NewUser, RepositoryError, UserRepository, UserRepositoryImpl},
};

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
    Sqlx(#[from] RepositoryError),

    #[error(transparent)]
    Reqwest(reqwest::Error),

    #[error(transparent)]
    OAuth2(BasicRequestTokenError<HttpClientError<reqwest::Error>>),
}

type AuthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

// OAuth2 5's built-in transport targets Reqwest 0.12; adapt its HTTP types to our
// shared client so token exchange uses the same redirect policy as user-info calls.
async fn oauth_http_request(
    client: reqwest::Client,
    request: HttpRequest,
) -> Result<HttpResponse, HttpClientError<reqwest::Error>> {
    let response = client
        .execute(request.try_into().map_err(Box::new)?)
        .await
        .map_err(Box::new)?;
    let mut builder = oauth2::http::Response::builder()
        .status(response.status())
        .version(response.version());
    for (name, value) in response.headers() {
        builder = builder.header(name, value);
    }
    builder
        .body(response.bytes().await.map_err(Box::new)?.to_vec())
        .map_err(HttpClientError::Http)
}

#[derive(Debug, Clone)]
pub struct AuthBackend {
    db: DbPool,
    client: AuthClient,
    http_client: reqwest::Client,
}

impl AuthBackend {
    pub fn new(db: DbPool, client: AuthClient) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build OAuth HTTP client");

        Self {
            db,
            client,
            http_client,
        }
    }

    pub fn authorize_url(&self) -> (Url, CsrfToken) {
        self.client.authorize_url(CsrfToken::new_random).url()
    }
}

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
            .request_async(&|request| oauth_http_request(self.http_client.clone(), request))
            .await
            .map_err(Self::Error::OAuth2)?;

        // Use access token to request user info.
        let user_info = self
            .http_client
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
        let user_repo = UserRepositoryImpl::new(self.db.clone());
        let new_user = NewUser::new(
            user_info.email,
            user_info.full_name,
            user_info.picture,
            token_res.access_token().secret().to_string(),
        );

        let user = user_repo.upsert_user(&new_user).await?;

        Ok(Some(user))
    }

    async fn get_user(
        &self,
        user_id: &SessionUserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user_id = UserId::from(*user_id as i32);
        let span = tracing::info_span!(
            "auth.get_user",
            operation.name = "auth.get_user",
            user.id = %user_id,
        );

        let user = async {
            let user_repo = UserRepositoryImpl::new(self.db.clone());
            user_repo.get_user(user_id).await
        }
        .instrument(span)
        .await?;

        Ok(Some(user))
    }
}

impl AuthzBackend for AuthBackend {
    type Permission = Role;

    async fn get_user_permissions(
        &self,
        user: &Self::User,
    ) -> Result<HashSet<Self::Permission>, Self::Error> {
        let perms = user.roles.iter().cloned().collect();
        Ok(perms)
    }
}

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post, Json, Router};
    use http_body_util::BodyExt;
    use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl};

    #[tokio::test]
    async fn token_exchange_preserves_form_body_and_client_auth() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let app = Router::new().route(
            "/token",
            post(|request: Request<Body>| async {
                assert_eq!(
                    request.headers()["authorization"],
                    "Basic Y2xpZW50OnNlY3JldA=="
                );
                assert_eq!(
                    request.headers()["content-type"],
                    "application/x-www-form-urlencoded"
                );
                let body = request.into_body().collect().await.unwrap().to_bytes();
                let fields: HashSet<(String, String)> =
                    url::form_urlencoded::parse(&body).into_owned().collect();
                assert!(fields.contains(&("grant_type".into(), "authorization_code".into())));
                assert!(fields.contains(&("code".into(), "a code & value".into())));
                Json(serde_json::json!({"access_token": "issued-token", "token_type": "Bearer"}))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let oauth = BasicClient::new(ClientId::new("client".into()))
            .set_client_secret(ClientSecret::new("secret".into()))
            .set_auth_uri(AuthUrl::new("http://localhost/authorize".into()).unwrap())
            .set_token_uri(TokenUrl::new(endpoint).unwrap());
        let result = oauth
            .exchange_code(AuthorizationCode::new("a code & value".into()))
            .request_async(&|request| oauth_http_request(client.clone(), request))
            .await;
        server.abort();
        assert_eq!(result.unwrap().access_token().secret(), "issued-token");
    }

    #[tokio::test]
    async fn oauth_transport_returns_redirects_without_following_them() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let app = Router::new().route(
            "/token",
            post(|| async {
                (
                    axum::http::StatusCode::FOUND,
                    [("location", "http://127.0.0.1:1/untrusted")],
                    "redirect response",
                )
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let request = oauth2::http::Request::builder()
            .method("POST")
            .uri(endpoint)
            .body(Vec::new())
            .unwrap();
        let result = oauth_http_request(client.clone(), request).await;
        server.abort();
        let response = result.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::FOUND);
        assert_eq!(
            response.headers()["location"],
            "http://127.0.0.1:1/untrusted"
        );
        assert_eq!(response.body(), b"redirect response");
    }
}
