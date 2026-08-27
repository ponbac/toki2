use crate::{
    adapters::inbound::http::{
        ErrorResponse, GetTimerResponse, SaveTimerResponse, TimeEntryResponse,
        TimerHistoryEntryResponse, TimerResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::models::ActiveTimer,
    routes::ApiError,
};

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use super::normalize_user_note;

use crate::observability::record_user_id;

// ============================================================================
// Get Timer
// ============================================================================

/// Get the active timer
///
/// Returns the authenticated user's currently running timer, or `null` when
/// none is running.
#[utoipa::path(
    get,
    path = "/time-tracking/timer",
    operation_id = "getActiveTimer",
    tag = "Time tracking",
    responses(
        (status = 200, description = "Active timer, if any", body = GetTimerResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "User is not connected to a time tracking provider", body = ErrorResponse),
        (status = 500, description = "Timer lookup failed", body = ErrorResponse),
        (status = 503, description = "Time tracking integration is unavailable", body = ErrorResponse)
    )
)]
pub async fn get_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<GetTimerResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let active_timer = service.get_active_timer(&user.id).await?;

    Ok(Json(GetTimerResponse {
        timer: active_timer.map(TimerResponse::from),
    }))
}

// ============================================================================
// Start Timer
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerPayload {
    user_note: Option<String>,
    /// Project ID. Provide together with `projectName`; omit both for no project.
    project_id: Option<String>,
    /// Project name. Provide together with `projectId`; omit both for no project.
    project_name: Option<String>,
    /// Activity ID. Provide together with `activityName`; omit both for no activity.
    activity_id: Option<String>,
    /// Activity name. Provide together with `activityId`; omit both for no activity.
    activity_name: Option<String>,
}

/// Start a timer
///
/// Starts a timer for the authenticated user. Project and activity selections
/// are optional; each selection requires both its ID and name.
#[utoipa::path(
    post,
    path = "/time-tracking/timer",
    operation_id = "startActiveTimer",
    tag = "Time tracking",
    request_body = StartTimerPayload,
    responses(
        (status = 200, description = "Timer started"),
        (status = 400, description = "Invalid timer details", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "A timer is already running", body = ErrorResponse),
        (status = 500, description = "Timer could not be started", body = ErrorResponse)
    )
)]
pub async fn start_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let mut timer = ActiveTimer::new(OffsetDateTime::now_utc());

    if let (Some(pid), Some(pname)) = (body.project_id, body.project_name) {
        timer = timer.with_project(pid, pname);
    }
    if let (Some(aid), Some(aname)) = (body.activity_id, body.activity_name) {
        timer = timer.with_activity(aid, aname);
    }
    if let Some(note) = body.user_note {
        let note = normalize_user_note(note);
        timer = timer.with_note(note);
    }

    service.start_timer(&user.id, &timer).await?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Stop Timer
// ============================================================================

/// Stop the active timer
///
/// Stops the authenticated user's timer without creating a time entry.
#[utoipa::path(
    delete,
    path = "/time-tracking/timer",
    operation_id = "stopActiveTimer",
    tag = "Time tracking",
    responses(
        (status = 200, description = "Timer stopped"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "No active timer found", body = ErrorResponse),
        (status = 500, description = "Timer could not be stopped", body = ErrorResponse)
    )
)]
pub async fn stop_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    service.stop_timer(&user.id).await?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Save Timer (pushes to provider via service layer)
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerPayload {
    user_note: Option<String>,
    restart_timer: Option<RestartTimerPayload>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestartTimerPayload {
    user_note: String,
    /// Project ID. Provide together with `projectName`; omit both for no project.
    project_id: Option<String>,
    /// Project name. Provide together with `projectId`; omit both for no project.
    project_name: Option<String>,
    /// Activity ID. Provide together with `activityName`; omit both for no activity.
    activity_id: Option<String>,
    /// Activity name. Provide together with `activityId`; omit both for no activity.
    activity_name: Option<String>,
}

/// Save the active timer
///
/// Creates a time entry from the authenticated user's active timer and can
/// optionally start a replacement timer.
#[utoipa::path(
    put,
    path = "/time-tracking/timer",
    operation_id = "saveActiveTimer",
    tag = "Time tracking",
    request_body = SaveTimerPayload,
    responses(
        (status = 200, description = "Timer saved", body = SaveTimerResponse),
        (status = 400, description = "Invalid timer details", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "No active timer found", body = ErrorResponse),
        (status = 409, description = "The time entry conflicts with provider state", body = ErrorResponse),
        (status = 500, description = "Timer could not be saved", body = ErrorResponse)
    )
)]
pub async fn save_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<SaveTimerPayload>,
) -> Result<Json<SaveTimerResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let user_note = body.user_note.map(normalize_user_note);

    let entry = service.save_timer(&user.id, user_note).await?;

    let timer = if let Some(restart_timer) = body.restart_timer {
        let mut timer = ActiveTimer::new(OffsetDateTime::now_utc())
            .with_note(normalize_user_note(restart_timer.user_note));

        if let (Some(project_id), Some(project_name)) =
            (restart_timer.project_id, restart_timer.project_name)
        {
            timer = timer.with_project(project_id, project_name);
        }
        if let (Some(activity_id), Some(activity_name)) =
            (restart_timer.activity_id, restart_timer.activity_name)
        {
            timer = timer.with_activity(activity_id, activity_name);
        }

        service.start_timer(&user.id, &timer).await?;
        Some(TimerResponse::from(timer))
    } else {
        None
    };

    Ok(Json(SaveTimerResponse {
        entry: TimeEntryResponse::from(entry),
        timer,
    }))
}

// ============================================================================
// Edit Timer
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
    /// RFC 3339 timestamp for the adjusted timer start.
    #[schema(value_type = Option<String>, format = "date-time")]
    start_time: Option<String>,
}

/// Update the active timer
///
/// Updates the supplied fields on the authenticated user's active timer.
#[utoipa::path(
    put,
    path = "/time-tracking/update-timer",
    operation_id = "updateActiveTimer",
    tag = "Time tracking",
    request_body = EditTimerPayload,
    responses(
        (status = 200, description = "Timer updated"),
        (status = 400, description = "Invalid timer details", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "No active timer found", body = ErrorResponse),
        (status = 500, description = "Timer could not be updated", body = ErrorResponse)
    )
)]
pub async fn edit_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<EditTimerPayload>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    // Get the current timer to merge with edits
    let current_timer = service
        .get_active_timer(&user.id)
        .await?
        .ok_or_else(|| ApiError::not_found("no active timer found"))?;

    let parsed_start_time: Option<OffsetDateTime> = body
        .start_time
        .filter(|st_iso_str| !st_iso_str.is_empty())
        .map(|st_iso_str| {
            OffsetDateTime::parse(&st_iso_str, &time::format_description::well_known::Rfc3339)
                .map_err(|e| {
                    tracing::warn!(
                        "Failed to parse start_time ISO string '{}': {}",
                        st_iso_str,
                        e
                    );
                    ApiError::bad_request(format!(
                        "Invalid start_time format. Expected ISO 8601 string. Details: {}",
                        e
                    ))
                })
        })
        .transpose()?;

    let mut updated_timer = ActiveTimer::new(parsed_start_time.unwrap_or(current_timer.started_at));

    // Merge: use provided values or fall back to current timer
    if let (Some(pid), Some(pname)) = (
        body.project_id
            .or(current_timer.project_id.map(|p| p.to_string())),
        body.project_name.or(current_timer.project_name),
    ) {
        updated_timer = updated_timer.with_project(pid, pname);
    }

    if let (Some(aid), Some(aname)) = (
        body.activity_id
            .or(current_timer.activity_id.map(|a| a.to_string())),
        body.activity_name.or(current_timer.activity_name),
    ) {
        updated_timer = updated_timer.with_activity(aid, aname);
    }

    let note = body
        .user_note
        .map(normalize_user_note)
        .unwrap_or(current_timer.note);
    updated_timer = updated_timer.with_note(note);

    service.edit_timer(&user.id, &updated_timer).await?;

    Ok(StatusCode::OK)
}

// ============================================================================
// Timer History
// ============================================================================

/// List timer history
///
/// Returns recent timer intervals for the authenticated user, including
/// unsaved and saved runs.
#[utoipa::path(
    get,
    path = "/time-tracking/timer-history",
    operation_id = "listTimerHistory",
    tag = "Time tracking",
    responses(
        (status = 200, description = "Timer history entries", body = Vec<TimerHistoryEntryResponse>),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn get_timer_history(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<TimerHistoryEntryResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let entries = service.get_timer_history(&user.id).await?;
    Ok(Json(entries.into_iter().map(Into::into).collect()))
}
