use axum::{
    extract::{Path, State},
    http::{header::CACHE_CONTROL, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    app_state::AppState,
    auth::AuthUser,
    domain::models::{ApiToken, ApiTokenId, IssuedApiToken},
    routes::ApiError,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tokens).post(create_token))
        .route("/:token_id", delete(revoke_token))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTokenRequest {
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiTokenResponse {
    id: i32,
    name: String,
    prefix: String,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedApiTokenResponse {
    #[serde(flatten)]
    metadata: ApiTokenResponse,
    token: String,
}

impl From<&ApiToken> for ApiTokenResponse {
    fn from(token: &ApiToken) -> Self {
        Self {
            id: token.id.as_i32(),
            name: token.name.as_str().to_string(),
            prefix: token.prefix.clone(),
            created_at: token.created_at,
        }
    }
}

impl From<IssuedApiToken> for CreatedApiTokenResponse {
    fn from(issued: IssuedApiToken) -> Self {
        Self {
            token: issued.secret.as_str().to_string(),
            metadata: ApiTokenResponse::from(&issued.token),
        }
    }
}

async fn list_tokens(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<ApiTokenResponse>>, ApiError> {
    let tokens = app_state.api_token_service.list(&user.id).await?;
    Ok(Json(tokens.iter().map(ApiTokenResponse::from).collect()))
}

async fn create_token(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Response, ApiError> {
    let issued = app_state
        .api_token_service
        .create(&user.id, &body.name)
        .await?;
    let mut response = (
        StatusCode::CREATED,
        Json(CreatedApiTokenResponse::from(issued)),
    )
        .into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn revoke_token(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(token_id): Path<i32>,
) -> Result<StatusCode, ApiError> {
    app_state
        .api_token_service
        .revoke(&user.id, &ApiTokenId::new(token_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
