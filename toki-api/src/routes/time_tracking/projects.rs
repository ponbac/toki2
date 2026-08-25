use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    adapters::inbound::http::{ActivityResponse, ErrorResponse, ProjectResponse},
    app_state::AppState,
    auth::AuthUser,
    observability::{record_span_field, record_user_id},
    routes::ApiError,
};

/// List time-tracking projects
///
/// Returns projects the authenticated user can book time against.
#[utoipa::path(
    get,
    path = "/time-tracking/projects",
    operation_id = "listTimeTrackingProjects",
    tag = "Time tracking",
    responses(
        (status = 200, description = "Projects available for time booking", body = Vec<ProjectResponse>),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
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

/// List activities for a project
///
/// Returns bookable activities for one project, filtered to the current date.
#[utoipa::path(
    get,
    path = "/time-tracking/projects/{project_id}/activities",
    operation_id = "listTimeTrackingActivities",
    tag = "Time tracking",
    params(
        ("project_id" = String, Path, description = "Time-tracking project identifier")
    ),
    responses(
        (status = 200, description = "Activities for the project", body = Vec<ActivityResponse>),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
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
