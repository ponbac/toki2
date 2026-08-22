use std::sync::Arc;

use axum::{
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    auth::{extractor::ApiTokenPrincipal, AuthSession},
    domain::models::ApiTokenCapability,
    domain::ports::inbound::ApiTokenAuthenticator,
    routes::ApiError,
};

/// Resolves an explicit Bearer credential into a narrow request principal.
///
/// Router composition applies this middleware to protected application routes.
/// An Authorization header is authoritative over a cookie session on those routes,
/// so mixed credentials cannot silently select another identity.
pub async fn authenticate_bearer(
    State(tokens): State<Arc<dyn ApiTokenAuthenticator>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let presented = match bearer_token(request.headers()) {
        Ok(Some(token)) => token,
        Ok(None) => return next.run(request).await,
        Err(()) => return bearer_unauthorized(),
    };

    match tokens.authenticate(presented).await {
        Ok(Some(grant)) => {
            request.extensions_mut().insert(ApiTokenPrincipal(grant));
            next.run(request).await
        }
        Ok(None) => bearer_unauthorized(),
        Err(error) => {
            tracing::error!(error = %error, "api token authentication failed");
            ApiError::from(error).into_response()
        }
    }
}

/// Guards application routes while accepting either a session or an API-token
/// principal produced by the outer authentication middleware.
pub async fn require_authenticated(request: Request<axum::body::Body>, next: Next) -> Response {
    let api_token_authenticated = request.extensions().get::<ApiTokenPrincipal>().is_some();
    let session_authenticated = request
        .extensions()
        .get::<AuthSession>()
        .and_then(|session| session.user.as_ref())
        .is_some();

    if api_token_authenticated || session_authenticated {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// Restricts API-token callers to a required capability. Session users pass through.
pub async fn require_capability(
    axum::extract::State(required): axum::extract::State<ApiTokenCapability>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match request.extensions().get::<ApiTokenPrincipal>() {
        Some(ApiTokenPrincipal(grant)) if grant.capabilities.contains(required) => {
            next.run(request).await
        }
        Some(_) => ApiError::forbidden("token is missing required capability").into_response(),
        None => next.run(request).await,
    }
}

fn bearer_token(headers: &HeaderMap) -> Result<Option<&str>, ()> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ())?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let scheme = parts.next().ok_or(())?;
    let token = parts
        .next()
        .map(str::trim)
        .filter(|token| !token.is_empty());

    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(());
    }

    token.map(Some).ok_or(())
}

fn bearer_unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use axum::{body::Body, middleware, routing::get, Router};
    use tower::ServiceExt;

    use crate::{
        auth::AuthUser,
        domain::{
            models::{ApiTokenCapabilities, ApiTokenCapability, ApiTokenGrant},
            ApiTokenError, Role, UserPrincipal,
        },
    };

    use super::*;

    const TIMER_STATUS_PATH: &str = "/time-tracking/timer";

    struct StubAuthenticator {
        grant: Option<ApiTokenGrant>,
    }

    #[async_trait]
    impl ApiTokenAuthenticator for StubAuthenticator {
        async fn authenticate(
            &self,
            _presented: &str,
        ) -> Result<Option<ApiTokenGrant>, ApiTokenError> {
            Ok(self.grant.clone())
        }
    }

    fn app(grant: Option<ApiTokenGrant>) -> Router {
        let authenticator: Arc<dyn ApiTokenAuthenticator> = Arc::new(StubAuthenticator { grant });

        Router::new()
            .route(
                TIMER_STATUS_PATH,
                get(|user: AuthUser| async move { user.id.to_string() }),
            )
            .route_layer(middleware::from_fn_with_state(
                ApiTokenCapability::TimerRead,
                require_capability,
            ))
            .route_layer(middleware::from_fn(require_authenticated))
            .layer(middleware::from_fn_with_state(
                authenticator,
                authenticate_bearer,
            ))
    }

    fn principal() -> UserPrincipal {
        UserPrincipal {
            id: crate::domain::models::UserId::new(7),
            email: "ada@example.com".to_string(),
            roles: vec![Role::User],
        }
    }

    fn grant(capabilities: ApiTokenCapabilities) -> ApiTokenGrant {
        ApiTokenGrant {
            principal: principal(),
            capabilities,
        }
    }

    #[test]
    fn parses_bearer_scheme_case_insensitively() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "bearer  toki_secret  ".parse().unwrap());
        assert_eq!(bearer_token(&headers), Ok(Some("toki_secret")));
    }

    #[tokio::test]
    async fn valid_token_authenticates_timer_status_request() {
        let response = app(Some(grant(ApiTokenCapabilities::timer_read_only())))
            .oneshot(
                Request::builder()
                    .uri(TIMER_STATUS_PATH)
                    .header(AUTHORIZATION, "Bearer toki_secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn token_missing_required_capability_is_forbidden() {
        let catalog_only = ApiTokenCapabilities::parse(["catalog:read"]).expect("known capability");
        let response = app(Some(grant(catalog_only)))
            .oneshot(
                Request::builder()
                    .uri(TIMER_STATUS_PATH)
                    .header(AUTHORIZATION, "Bearer toki_secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unknown_token_is_unauthorized() {
        let response = app(None)
            .oneshot(
                Request::builder()
                    .uri(TIMER_STATUS_PATH)
                    .header(AUTHORIZATION, "Bearer toki_unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.headers()[axum::http::header::WWW_AUTHENTICATE],
            "Bearer"
        );
    }
}
