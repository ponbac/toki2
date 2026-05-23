use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    adapters::inbound::http::{ActivityResponse, ProjectResponse},
    app_state::AppState,
    auth::AuthUser,
    observability::{record_span_field, record_user_id},
    routes::ApiError,
};

pub async fn list_projects(
    State(app_state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ProjectResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let projects = service.get_projects().await?;

    Ok(Json(
        projects.into_iter().map(ProjectResponse::from).collect(),
    ))
}

pub async fn list_activities(
    Path(project_id): Path<String>,
    State(app_state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ActivityResponse>>, ApiError> {
    record_user_id(user.id);
    record_span_field("project.id", &project_id);
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
