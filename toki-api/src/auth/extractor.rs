use std::ops::Deref;

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};

use crate::{domain::models::UserId, domain::User, routes::ApiError};

use super::AuthSession;

/// A custom Axum extractor that extracts the authenticated [`User`] directly
/// from the request. Returns 401 Unauthorized if no user is logged in.
///
/// This replaces the pattern of extracting `AuthSession` and then manually
/// unwrapping `.user` with `.expect()` or `.ok_or()` in every handler.
///
/// The `id` field is a [`UserId`] constructed at extraction time, shadowing
/// `User.id` (which is `i32`) through `Deref`. Handlers that need `i32` can
/// use `user.id.as_i32()`.
///
/// Safe to log — `User`'s `Debug` impl redacts sensitive fields.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: UserId,
    user: User,
}

impl Deref for AuthUser {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.user
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthSession: FromRequestParts<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_session = AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::unauthorized("Not authenticated"))?;

        let user = auth_session
            .user
            .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?;

        Ok(AuthUser {
            id: UserId::from(user.id),
            user,
        })
    }
}
