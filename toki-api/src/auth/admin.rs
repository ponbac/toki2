use axum::{
    body::Body,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::routes::ApiError;

use super::AuthUser;

/// Guards admin routes: only a caller with admin power, an admin in a browser
/// session (`AuthUser::has_admin_power`), gets through. Others get `403`, and
/// unauthenticated requests `401`. Because the bearer principal takes
/// precedence, a request with an API token and an admin's session cookie is
/// refused too.
pub async fn require_admin_power(user: AuthUser, request: Request<Body>, next: Next) -> Response {
    if !user.has_admin_power() {
        return ApiError::forbidden(
            "only an admin signed in to Toki, not using an API token, can use admin routes",
        )
        .into_response();
    }

    next.run(request).await
}
