use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    adapters::inbound::http::ErrorResponse,
    app_state::AppState,
    auth::AuthUser,
    domain::{
        models::KLEER_TIME_TRACKING_PROVIDER, ports::outbound::TimeTrackingUserLinkRepository,
    },
    observability::record_user_id,
    repositories::TimeTrackingUserLinkRepositoryImpl,
    routes::ApiError,
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusResponse {
    connected: bool,
    provider_user_id: Option<String>,
    provider_user_email: Option<String>,
    provider_user_name: Option<String>,
}

/// Get time-tracking connection status
///
/// Returns whether the authenticated user is mapped to a time-tracking
/// provider user, plus the mapped identity when connected.
#[utoipa::path(
    get,
    path = "/time-tracking/connection",
    operation_id = "getTimeTrackingConnection",
    tag = "Time tracking",
    responses(
        (status = 200, description = "Connection status for the current user", body = ConnectionStatusResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn connection_status(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<axum::Json<ConnectionStatusResponse>, ApiError> {
    record_user_id(user.id);
    let credentials = app_state
        .kleer_settings
        .credentials()
        .map_err(|error| ApiError::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, error))?;
    let repo = TimeTrackingUserLinkRepositoryImpl::new(app_state.db_pool.clone());
    let link = repo
        .get_active_link_for_user(&user.id, KLEER_TIME_TRACKING_PROVIDER)
        .await?
        .filter(|link| link.provider_company_id == credentials.company_id);

    Ok(axum::Json(match link {
        Some(link) => ConnectionStatusResponse {
            connected: true,
            provider_user_id: Some(link.provider_user_id),
            provider_user_email: link.provider_user_email,
            provider_user_name: link.provider_user_name,
        },
        None => ConnectionStatusResponse {
            connected: false,
            provider_user_id: None,
            provider_user_email: None,
            provider_user_name: None,
        },
    }))
}
