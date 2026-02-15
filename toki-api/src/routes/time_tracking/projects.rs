use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::CookieJar;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{ActivityResponse, ProjectResponse},
    app_state::AppState,
};

use super::CookieJarResult;

#[instrument(name = "list_projects", skip(jar))]
pub async fn list_projects(
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<ProjectResponse>>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let projects = service.get_projects().await?;

    let response: Vec<ProjectResponse> = projects.into_iter().map(ProjectResponse::from).collect();

    Ok((jar, Json(response)))
}

#[instrument(name = "list_activities", skip(jar))]
pub async fn list_activities(
    Path(project_id): Path<String>,
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<ActivityResponse>>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    // Use current date range for activity filtering (matches old behavior)
    let today = time::OffsetDateTime::now_utc().date();
    let date_range = (today, today);

    let activities = service
        .get_activities(&project_id.into(), date_range)
        .await?;

    let response: Vec<ActivityResponse> =
        activities.into_iter().map(ActivityResponse::from).collect();

    Ok((jar, Json(response)))
}
