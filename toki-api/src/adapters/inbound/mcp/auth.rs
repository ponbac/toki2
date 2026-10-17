//! Bearer authentication for the MCP endpoint.
//!
//! The endpoint accepts a Toki personal API token, or, when OAuth is
//! configured, a Microsoft Entra ID access token issued for it. Browser
//! session cookies are never accepted, so a web page cannot drive an agent
//! surface with a signed-in user's cookie. Neither kind of token carries admin
//! power.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    http::{header::WWW_AUTHENTICATE, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use super::entra::{EntraTokenError, EntraTokenValidator};
use crate::{
    auth::{bearer_token, AuthUser},
    domain::{ports::inbound::ApiTokenAuthenticator, AuthMethod, UserPrincipal},
    repositories::{RepositoryError, UserRepository, UserRepositoryImpl},
};

const API_TOKEN_PREFIX: &str = "toki_";

/// Finds the Toki user an access token names.
#[async_trait]
pub trait McpUserDirectory: Send + Sync {
    async fn find_by_email(&self, email: &str) -> Result<Option<UserPrincipal>, RepositoryError>;
}

#[async_trait]
impl McpUserDirectory for UserRepositoryImpl {
    async fn find_by_email(&self, email: &str) -> Result<Option<UserPrincipal>, RepositoryError> {
        Ok(self
            .find_user_by_email(email)
            .await?
            .map(|user| UserPrincipal::from(&user)))
    }
}

/// OAuth 2.0 Protected Resource Metadata (RFC 9728) for the MCP endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub bearer_methods_supported: Vec<String>,
    pub resource_name: String,
}

pub struct McpOAuth {
    pub validator: EntraTokenValidator,
    pub users: Arc<dyn McpUserDirectory>,
    pub metadata: ProtectedResourceMetadata,
    /// Where `metadata` is served.
    pub metadata_url: String,
}

pub struct McpAuth {
    pub api_tokens: Arc<dyn ApiTokenAuthenticator>,
    pub oauth: Option<McpOAuth>,
}

enum Rejection {
    /// No usable credential: the client should (re)authenticate.
    Unauthenticated {
        error: Option<&'static str>,
    },
    /// A valid token without the endpoint's scope.
    InsufficientScope,
    /// A valid token for someone Toki does not know.
    UnknownUser,
    Unavailable(String),
}

impl McpAuth {
    async fn resolve(&self, token: &str) -> Result<AuthUser, Rejection> {
        if token.starts_with(API_TOKEN_PREFIX) {
            return match self.api_tokens.authenticate(token).await {
                Ok(Some(principal)) => Ok(AuthUser::new(principal, AuthMethod::ApiToken)),
                Ok(None) => Err(Rejection::Unauthenticated {
                    error: Some("invalid_token"),
                }),
                Err(error) => {
                    tracing::error!(error = %error, "mcp api token authentication failed");
                    Err(Rejection::Unavailable("Authentication failed.".into()))
                }
            };
        }

        let Some(oauth) = &self.oauth else {
            return Err(Rejection::Unauthenticated {
                error: Some("invalid_token"),
            });
        };
        let identity = oauth.validator.validate(token).await.map_err(|error| {
            tracing::info!(error = %error, "mcp access token rejected");
            match error {
                EntraTokenError::Invalid(_) => Rejection::Unauthenticated {
                    error: Some("invalid_token"),
                },
                EntraTokenError::MissingScope(_) => Rejection::InsufficientScope,
                EntraTokenError::Keys(message) => Rejection::Unavailable(message),
            }
        })?;
        for email in &identity.emails {
            match oauth.users.find_by_email(email).await {
                Ok(Some(principal)) => {
                    return Ok(AuthUser::new(principal, AuthMethod::AccessToken))
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(error = %error, "mcp user lookup failed");
                    return Err(Rejection::Unavailable("Authentication failed.".into()));
                }
            }
        }
        Err(Rejection::UnknownUser)
    }

    fn challenge(&self, error: Option<&str>) -> HeaderValue {
        let mut parameters = Vec::new();
        if let Some(oauth) = &self.oauth {
            parameters.push(format!("resource_metadata=\"{}\"", oauth.metadata_url));
            parameters.push(format!(
                "scope=\"{}\"",
                oauth.metadata.scopes_supported.join(" ")
            ));
        }
        if let Some(error) = error {
            parameters.push(format!("error=\"{error}\""));
        }
        let challenge = if parameters.is_empty() {
            "Bearer".to_string()
        } else {
            format!("Bearer {}", parameters.join(", "))
        };
        HeaderValue::from_str(&challenge).unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
    }

    fn reject(&self, rejection: Rejection) -> Response {
        let (status, error, message) = match rejection {
            Rejection::Unauthenticated { error } => (
                StatusCode::UNAUTHORIZED,
                error,
                "Present a Toki API token or an access token for this server.".to_string(),
            ),
            Rejection::InsufficientScope => (
                StatusCode::FORBIDDEN,
                Some("insufficient_scope"),
                "The access token lacks this server's scope.".to_string(),
            ),
            Rejection::UnknownUser => (
                StatusCode::FORBIDDEN,
                None,
                "No Toki user has this account. Sign in to Toki in the browser once first."
                    .to_string(),
            ),
            Rejection::Unavailable(message) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ErrorBody { error: message }),
                )
                    .into_response()
            }
        };
        let mut response = (status, Json(ErrorBody { error: message })).into_response();
        if status == StatusCode::UNAUTHORIZED || error.is_some() {
            response
                .headers_mut()
                .insert(WWW_AUTHENTICATE, self.challenge(error));
        }
        response
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

/// Resolves the request's bearer credential into an [`AuthUser`] extension
/// that the MCP tools read.
pub async fn authenticate(
    State(auth): State<Arc<McpAuth>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = match bearer_token(request.headers()) {
        Ok(Some(token)) => token.to_owned(),
        Ok(None) => return auth.reject(Rejection::Unauthenticated { error: None }),
        Err(()) => {
            return auth.reject(Rejection::Unauthenticated {
                error: Some("invalid_request"),
            })
        }
    };

    match auth.resolve(&token).await {
        Ok(user) => {
            crate::observability::record_user_id(user.id);
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Err(rejection) => auth.reject(rejection),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use tower::ServiceExt;

    use super::*;
    use crate::domain::{models::UserId, ApiTokenError, Role};

    struct StubTokens;

    #[async_trait]
    impl ApiTokenAuthenticator for StubTokens {
        async fn authenticate(
            &self,
            presented: &str,
        ) -> Result<Option<UserPrincipal>, ApiTokenError> {
            Ok((presented == "toki_valid").then(principal))
        }
    }

    struct NoUsers;

    #[async_trait]
    impl McpUserDirectory for NoUsers {
        async fn find_by_email(
            &self,
            _email: &str,
        ) -> Result<Option<UserPrincipal>, RepositoryError> {
            Ok(None)
        }
    }

    fn principal() -> UserPrincipal {
        UserPrincipal {
            id: UserId::new(7),
            email: "ada@example.com".to_string(),
            roles: vec![Role::Admin],
        }
    }

    fn oauth() -> McpOAuth {
        McpOAuth {
            validator: EntraTokenValidator::new("tenant", "audience", "mcp"),
            users: Arc::new(NoUsers),
            metadata: ProtectedResourceMetadata {
                resource: "https://toki.example/mcp".into(),
                authorization_servers: vec!["https://login.example/tenant/v2.0".into()],
                scopes_supported: vec!["https://toki.example/mcp/mcp".into()],
                bearer_methods_supported: vec!["header".into()],
                resource_name: "Toki".into(),
            },
            metadata_url: "https://toki.example/.well-known/oauth-protected-resource/mcp".into(),
        }
    }

    fn app(oauth: Option<McpOAuth>) -> Router {
        let auth = Arc::new(McpAuth {
            api_tokens: Arc::new(StubTokens),
            oauth,
        });
        Router::new()
            .route(
                "/mcp",
                get(|request: Request<Body>| async move {
                    let user = request.extensions().get::<AuthUser>().unwrap();
                    format!("{} {}", user.id, user.has_admin_power())
                }),
            )
            .layer(middleware::from_fn_with_state(auth, authenticate))
    }

    async fn call(app: Router, authorization: Option<&str>) -> Response {
        let mut request = Request::builder().uri("/mcp");
        if let Some(authorization) = authorization {
            request = request.header("authorization", authorization);
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn an_api_token_authenticates_without_admin_power() {
        let response = call(app(None), Some("Bearer toki_valid")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, "7 false");
    }

    #[tokio::test]
    async fn a_missing_credential_points_oauth_clients_at_the_metadata() {
        let response = call(app(Some(oauth())), None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[WWW_AUTHENTICATE],
            "Bearer resource_metadata=\"https://toki.example/.well-known/oauth-protected-resource/mcp\", scope=\"https://toki.example/mcp/mcp\""
        );
    }

    #[tokio::test]
    async fn without_oauth_the_challenge_names_only_the_scheme() {
        let response = call(app(None), None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[WWW_AUTHENTICATE], "Bearer");
    }

    #[tokio::test]
    async fn unknown_and_malformed_credentials_are_rejected() {
        for (oauth, authorization) in [
            (None, "Bearer toki_unknown"),
            (None, "Bearer eyJhbGciOiJSUzI1NiJ9.e30.c2ln"),
            (Some(oauth()), "Bearer not-a-jwt"),
            (Some(oauth()), "Basic dXNlcjpwYXNz"),
        ] {
            let response = call(app(oauth), Some(authorization)).await;

            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{authorization}"
            );
            assert!(response.headers().contains_key(WWW_AUTHENTICATE));
        }
    }

    #[tokio::test]
    async fn a_session_cookie_is_not_a_credential() {
        let response = app(None)
            .oneshot(
                Request::builder()
                    .uri("/mcp")
                    .header("cookie", "toki.sid=session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
