use std::ops::Deref;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    domain::{AuthMethod, Role, UserPrincipal},
    routes::ApiError,
};

use super::AuthSession;

/// The narrow authenticated identity exposed to request handlers, and how it
/// was authenticated.
///
/// Provider credentials and session hashes cannot cross this boundary.
#[derive(Debug, Clone)]
pub struct AuthUser {
    principal: UserPrincipal,
    method: AuthMethod,
}

impl AuthUser {
    /// Whether the request came from a browser session or an API token. Routes
    /// may withhold powers from long-lived tokens that sessions have.
    pub fn method(&self) -> AuthMethod {
        self.method
    }

    /// Whether the caller may act as an admin: an admin in a browser session
    /// (`AuthMethod::allows_admin_power`).
    pub fn has_admin_power(&self) -> bool {
        self.roles.contains(&Role::Admin) && self.method.allows_admin_power()
    }
}

impl Deref for AuthUser {
    type Target = UserPrincipal;

    fn deref(&self) -> &Self::Target {
        &self.principal
    }
}

/// Request extension set only after successful API-token authentication.
#[derive(Debug, Clone)]
pub(super) struct ApiTokenPrincipal(pub UserPrincipal);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthSession: FromRequestParts<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(principal) = parts.extensions.get::<ApiTokenPrincipal>() {
            return Ok(Self {
                principal: principal.0.clone(),
                method: AuthMethod::ApiToken,
            });
        }

        let auth_session = AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::unauthorized("Not authenticated"))?;

        let user = auth_session
            .user
            .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?;

        Ok(Self {
            principal: UserPrincipal::from(&user),
            method: AuthMethod::Session,
        })
    }
}
