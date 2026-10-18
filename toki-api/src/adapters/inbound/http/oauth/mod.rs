//! HTTP adapter of Toki's OAuth 2.1 authorization server, through which MCP
//! clients connect to `/mcp`: metadata (RFC 8414), authorization with a
//! consent page, the token endpoint, dynamic client registration (RFC 7591)
//! and revocation (RFC 7009), plus the user's list of connected apps.

mod authorize;
mod grants;
mod metadata;
mod pages;
mod params;
mod register;
#[cfg(test)]
mod tests;
mod token;

use std::sync::Arc;

use axum::{
    http::{
        header::{CACHE_CONTROL, PRAGMA, WWW_AUTHENTICATE},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::domain::ports::inbound::OAuthService;

pub use grants::grants_router;
use register::RegistrationLimiter;

pub(super) const AUTHORIZE_PATH: &str = "/oauth/authorize";
const METADATA_PATH: &str = "/.well-known/oauth-authorization-server";
const TOKEN_PATH: &str = "/oauth/token";
const REGISTER_PATH: &str = "/oauth/register";
const REVOKE_PATH: &str = "/oauth/revoke";

#[derive(Debug, Clone)]
pub struct OAuthServerConfig {
    /// The issuer identifier: the API URL without a trailing slash. Endpoint
    /// URLs are built from it.
    pub issuer: String,
    /// The API's origin, such as `https://toki-api.spinit.se`, from which
    /// consent decisions must be posted.
    pub api_origin: String,
}

impl OAuthServerConfig {
    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.issuer)
    }
}

#[derive(Clone)]
struct ServerState {
    service: Arc<dyn OAuthService>,
    config: Arc<OAuthServerConfig>,
    registrations: Arc<RegistrationLimiter>,
}

/// The authorization server's endpoints. Mounted inside the session layer:
/// authorization signs the user in through Toki's own sign-in.
pub fn server_router<S>(service: Arc<dyn OAuthService>, config: OAuthServerConfig) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = ServerState {
        service,
        config: Arc::new(config),
        registrations: Arc::new(RegistrationLimiter::hourly()),
    };
    Router::new()
        .route(METADATA_PATH, get(metadata::metadata))
        .route(
            AUTHORIZE_PATH,
            get(authorize::authorize).post(authorize::decide),
        )
        .route(TOKEN_PATH, post(token::token))
        .route(REGISTER_PATH, post(register::register))
        .route(REVOKE_PATH, post(token::revoke))
        .route(pages::LOGO_PATH, get(pages::logo))
        .with_state(state)
}

/// An OAuth error response body (RFC 6749, section 5.2).
#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    error_description: &'a str,
}

/// A JSON protocol error. Like every token endpoint response, it must not be
/// cached.
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    no_store(
        (
            status,
            Json(ErrorBody {
                error,
                error_description: description,
            }),
        )
            .into_response(),
    )
}

/// `invalid_client`, challenging for Basic credentials when the client sent
/// them (RFC 6749, section 5.2).
fn invalid_client(description: &str, challenge_basic: bool) -> Response {
    let mut response = oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", description);
    if challenge_basic {
        response.headers_mut().insert(
            WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"toki\""),
        );
    }
    response
}

fn no_store(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
