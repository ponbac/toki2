//! Bearer authentication for the MCP endpoint.
//!
//! The endpoint accepts a Toki personal API token, or, with OAuth, an access
//! token Toki's own authorization server issued for it. Refresh tokens and
//! other OAuth secrets are not credentials here. Browser session cookies are
//! never accepted, so a web page cannot drive an agent surface with a
//! signed-in user's cookie. Neither kind of token carries admin power.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header::WWW_AUTHENTICATE, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::{
    auth::{bearer_token, AuthUser},
    domain::{
        ports::inbound::{ApiTokenAuthenticator, OAuthAccessTokenAuthenticator},
        AuthMethod,
    },
};

const API_TOKEN_PREFIX: &str = "toki_";
const ACCESS_TOKEN_PREFIX: &str = "tmcp_at_";

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
    pub access_tokens: Arc<dyn OAuthAccessTokenAuthenticator>,
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

        let invalid_token = Rejection::Unauthenticated {
            error: Some("invalid_token"),
        };
        let Some(oauth) = &self.oauth else {
            return Err(invalid_token);
        };
        if !token.starts_with(ACCESS_TOKEN_PREFIX) {
            return Err(invalid_token);
        }
        match oauth.access_tokens.authenticate(token).await {
            Ok(Some(principal)) => Ok(AuthUser::new(principal, AuthMethod::AccessToken)),
            Ok(None) => Err(invalid_token),
            Err(error) => {
                tracing::error!(error = %error, "mcp access token authentication failed");
                Err(Rejection::Unavailable("Authentication failed.".into()))
            }
        }
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
        match rejection {
            Rejection::Unauthenticated { error } => {
                let mut response = (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorBody {
                        error: "Present a Toki API token or an access token for this server."
                            .to_string(),
                    }),
                )
                    .into_response();
                response
                    .headers_mut()
                    .insert(WWW_AUTHENTICATE, self.challenge(error));
                response
            }
            Rejection::Unavailable(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorBody { error: message }),
            )
                .into_response(),
        }
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
    use async_trait::async_trait;
    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use tower::ServiceExt;

    use super::*;
    use crate::domain::{models::UserId, ApiTokenError, OAuthError, Role, UserPrincipal};

    const CHALLENGE: &str = "Bearer resource_metadata=\"https://toki.example/.well-known/oauth-protected-resource/mcp\", scope=\"mcp\"";

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

    /// Knows every secret ending in `valid`, whatever its kind, so the tests
    /// show which kinds `/mcp` refuses before asking.
    struct StubAccessTokens;

    #[async_trait]
    impl OAuthAccessTokenAuthenticator for StubAccessTokens {
        async fn authenticate(&self, presented: &str) -> Result<Option<UserPrincipal>, OAuthError> {
            if presented.ends_with("unavailable") {
                return Err(OAuthError::Storage("connection refused".to_string()));
            }
            Ok(presented.ends_with("valid").then(principal))
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
            access_tokens: Arc::new(StubAccessTokens),
            metadata: ProtectedResourceMetadata {
                resource: "https://toki.example/mcp".into(),
                authorization_servers: vec!["https://toki.example".into()],
                scopes_supported: vec!["mcp".into()],
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
        for oauth in [None, Some(oauth())] {
            let response = call(app(oauth), Some("Bearer toki_valid")).await;

            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(body(response).await, "7 false");
        }
    }

    #[tokio::test]
    async fn an_access_token_authenticates_without_admin_power() {
        let response = call(app(Some(oauth())), Some("Bearer tmcp_at_valid")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, "7 false");
    }

    #[tokio::test]
    async fn a_missing_credential_points_oauth_clients_at_the_metadata() {
        let response = call(app(Some(oauth())), None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[WWW_AUTHENTICATE], CHALLENGE);
    }

    #[tokio::test]
    async fn an_expired_or_unknown_access_token_asks_for_a_new_one() {
        let response = call(app(Some(oauth())), Some("Bearer tmcp_at_expired")).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[WWW_AUTHENTICATE],
            format!("{CHALLENGE}, error=\"invalid_token\"")
        );
    }

    #[tokio::test]
    async fn refresh_tokens_and_other_secrets_are_not_credentials() {
        for authorization in [
            "Bearer tmcp_rt_valid",
            "Bearer tmcp_ac_valid",
            "Bearer tmcp_ar_valid",
            "Bearer eyJhbGciOiJSUzI1NiJ9.e30.valid",
        ] {
            let response = call(app(Some(oauth())), Some(authorization)).await;

            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{authorization}"
            );
            assert_eq!(
                response.headers()[WWW_AUTHENTICATE],
                format!("{CHALLENGE}, error=\"invalid_token\""),
                "{authorization}"
            );
        }
    }

    #[tokio::test]
    async fn a_failed_access_token_lookup_is_unavailable() {
        let response = call(app(Some(oauth())), Some("Bearer tmcp_at_unavailable")).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.headers().contains_key(WWW_AUTHENTICATE));
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
            (None, "Bearer tmcp_at_valid"),
            (Some(oauth()), "Bearer toki_unknown"),
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
        let response = app(Some(oauth()))
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
