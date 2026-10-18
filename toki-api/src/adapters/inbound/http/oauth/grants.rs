//! The user's connected apps: the OAuth grants MCP clients hold.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::Serialize;
use time::OffsetDateTime;

use crate::{
    auth::AuthUser,
    domain::{
        models::{ClientIdUrl, OAuthGrant, OAuthGrantId, RedirectUri},
        ports::inbound::OAuthService,
        OAuthError,
    },
    routes::ApiError,
};

/// `GET /` lists the user's connected apps, `DELETE /{grant_id}` disconnects
/// one. Nested among the protected routes.
pub fn grants_router<S>(service: Arc<dyn OAuthService>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list_grants))
        .route("/{grant_id}", delete(revoke_grant))
        .with_state(service)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthGrantResponse {
    id: i32,
    client_name: String,
    /// The host that vouches for a client identified by a metadata document;
    /// `None` for a client that registered itself.
    client_host: Option<String>,
    /// Where the app received its authorization, as the consent page named it.
    redirect_destination: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    last_used_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
}

impl From<&OAuthGrant> for OAuthGrantResponse {
    fn from(grant: &OAuthGrant) -> Self {
        Self {
            id: grant.id.as_i32(),
            client_name: grant.client_name.clone(),
            client_host: ClientIdUrl::parse(&grant.client_id).map(|url| url.host().to_string()),
            redirect_destination: RedirectUri::parse(&grant.redirect_uri)
                .map(|uri| uri.display_destination())
                .unwrap_or_else(|| grant.redirect_uri.clone()),
            created_at: grant.created_at,
            last_used_at: grant.last_used_at,
            expires_at: grant.expires_at,
        }
    }
}

async fn list_grants(
    user: AuthUser,
    State(service): State<Arc<dyn OAuthService>>,
) -> Result<Json<Vec<OAuthGrantResponse>>, ApiError> {
    let grants = service.list_grants(&user.id).await.map_err(api_error)?;
    Ok(Json(grants.iter().map(OAuthGrantResponse::from).collect()))
}

async fn revoke_grant(
    user: AuthUser,
    State(service): State<Arc<dyn OAuthService>>,
    Path(grant_id): Path<i32>,
) -> Result<StatusCode, ApiError> {
    service
        .revoke_grant(&user.id, &OAuthGrantId::new(grant_id))
        .await
        .map_err(api_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn api_error(error: OAuthError) -> ApiError {
    match error {
        OAuthError::NotFound => ApiError::not_found("connected app not found"),
        OAuthError::Storage(message) => {
            tracing::error!("OAuth grant operation failed: {message}");
            ApiError::internal("connected app operation failed")
        }
        error => ApiError::bad_request(error.to_string()),
    }
}
