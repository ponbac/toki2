use std::ops::Deref;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{domain::UserPrincipal, routes::ApiError};

use super::AuthSession;

/// The narrow authenticated identity exposed to request handlers.
///
/// Provider credentials and session hashes cannot cross this boundary.
#[derive(Debug, Clone)]
pub struct AuthUser(UserPrincipal);

impl Deref for AuthUser {
    type Target = UserPrincipal;

    fn deref(&self) -> &Self::Target {
        &self.0
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
            return Ok(Self(principal.0.clone()));
        }

        let auth_session = AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::unauthorized("Not authenticated"))?;

        let user = auth_session
            .user
            .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?;

        Ok(Self(UserPrincipal::from(&user)))
    }
}
