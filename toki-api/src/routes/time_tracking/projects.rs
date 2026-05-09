use axum::{
    extract::{Path, State},
    Json,
};
use tracing::instrument;

use crate::{
    adapters::inbound::http::{ActivityResponse, ProjectResponse},
    app_state::AppState,
    auth::AuthUser,
    routes::ApiError,
};

#[instrument(name = "list_projects", skip(app_state))]
pub async fn list_projects(
    State(app_state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let projects = service.get_projects().await?;

    Ok(Json(
        projects.into_iter().map(ProjectResponse::from).collect(),
    ))
}

#[instrument(name = "list_activities", skip(app_state))]
pub async fn list_activities(
    Path(project_id): Path<String>,
    State(app_state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ActivityResponse>>, ApiError> {
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    // Use current date range for activity filtering (matches old behavior)
    let today = time::OffsetDateTime::now_utc().date();
    let activities = service
        .get_activities(&project_id.into(), (today, today))
        .await?;

    Ok(Json(
        activities.into_iter().map(ActivityResponse::from).collect(),
    ))
}
