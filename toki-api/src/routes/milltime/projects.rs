use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::CookieJar;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{
        ActivityResponse, ProjectResponse, TimeTrackingServiceError, TimeTrackingServiceExt,
    },
    app_state::AppState,
    domain::ports::inbound::TimeTrackingService,
};

use super::{CookieJarResult, ErrorResponse, MilltimeError};

#[instrument(name = "list_projects", skip(jar, app_state))]
pub async fn list_projects(
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<ProjectResponse>>> {
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    let projects = service
        .get_projects()
        .await
        .map_err(tracking_error_to_response)?;

    let response: Vec<ProjectResponse> = projects.into_iter().map(ProjectResponse::from).collect();

    Ok((jar, Json(response)))
}

#[instrument(name = "list_activities", skip(jar, app_state))]
pub async fn list_activities(
    Path(project_id): Path<String>,
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<ActivityResponse>>> {
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    // Use current date range for activity filtering (matches old behavior)
    let today = time::OffsetDateTime::now_utc().date();
    let date_range = (today, today);

    let activities = service
        .get_activities(&project_id.into(), date_range)
        .await
        .map_err(tracking_error_to_response)?;

    let response: Vec<ActivityResponse> = activities.into_iter().map(ActivityResponse::from).collect();

    Ok((jar, Json(response)))
}

fn service_error_to_response(e: TimeTrackingServiceError) -> ErrorResponse {
    ErrorResponse {
        status: e.status,
        error: MilltimeError::MilltimeAuthenticationFailed,
        message: e.message,
    }
}

fn tracking_error_to_response(e: crate::domain::TimeTrackingError) -> ErrorResponse {
    use crate::domain::TimeTrackingError;

    match e {
        TimeTrackingError::AuthenticationFailed => ErrorResponse {
            status: axum::http::StatusCode::UNAUTHORIZED,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: "Authentication failed".to_string(),
        },
        _ => ErrorResponse {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: e.to_string(),
        },
    }
}
