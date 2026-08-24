use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    adapters::inbound::http::ErrorResponse, app_state::AppState, auth::AuthUser,
    domain::models::TimeTrackingConnection, observability::record_user_id, routes::ApiError,
};

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusResponse {
    provider: String,
    connected: bool,
    provider_user_id: Option<String>,
    provider_user_email: Option<String>,
    provider_user_name: Option<String>,
}

impl From<TimeTrackingConnection> for ConnectionStatusResponse {
    fn from(connection: TimeTrackingConnection) -> Self {
        match connection {
            TimeTrackingConnection::Disconnected { provider } => Self {
                provider,
                connected: false,
                provider_user_id: None,
                provider_user_email: None,
                provider_user_name: None,
            },
            TimeTrackingConnection::Connected {
                provider,
                provider_user_id,
                provider_user_email,
                provider_user_name,
            } => Self {
                provider,
                connected: true,
                provider_user_id: Some(provider_user_id),
                provider_user_email,
                provider_user_name,
            },
        }
    }
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
    let connection = app_state
        .time_tracking_factory
        .connection_status(user.id)
        .await?;

    Ok(axum::Json(connection.into()))
}
