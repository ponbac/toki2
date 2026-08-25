use crate::{
    adapters::inbound::http::{
        ErrorResponse, GetTimerResponse, SaveTimerResponse, TimeEntryResponse,
        TimerHistoryEntryResponse, TimerResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::models::{ActivityId, PatchValue, ProjectId, StartTimerRequest, UpdateTimerRequest},
    routes::ApiError,
};

use axum::{
    extract::State,
    http::{header::HeaderName, HeaderMap, StatusCode},
    Json,
};
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartActiveTimerRequest {
    /// Optional project selection. The canonical name is resolved by Toki.
    project_id: Option<String>,
    /// Optional activity selection. Requires `projectId` and must be valid for it.
    activity_id: Option<String>,
    /// User-authored timer note.
    note: Option<String>,
}

/// Start an active timer
///
/// Starts the authenticated user's single active timer and returns the canonical
/// stored representation. Project and activity display names are resolved by Toki.
#[utoipa::path(
    post,
    path = "/time-tracking/timer",
    operation_id = "startActiveTimer",
    tag = "Time tracking",
    request_body = StartActiveTimerRequest,
    responses(
        (status = 201, description = "Timer started", body = TimerResponse),
        (status = 400, description = "Invalid project/activity selection", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "A timer is already running", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn start_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<StartActiveTimerRequest>,
) -> Result<(StatusCode, Json<TimerResponse>), ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = StartTimerRequest {
        project_id: body.project_id.map(ProjectId::new),
        activity_id: body.activity_id.map(ActivityId::new),
        note: normalize_user_note(body.note.unwrap_or_default()),
    };
    let timer = service.start_timer(&user.id, &request).await?;

    Ok((StatusCode::CREATED, Json(timer.into())))
}

// ============================================================================
// Stop Timer
// ============================================================================

/// Discard the active timer
///
/// Permanently discards the current timer without creating a time entry. This
/// is destructive and intentionally distinct from saving. Repeated calls are
/// idempotent and return `204` even when no timer is active.
#[utoipa::path(
    delete,
    path = "/time-tracking/timer",
    operation_id = "discardActiveTimer",
    tag = "Time tracking",
    responses(
        (status = 204, description = "Timer discarded or no timer was active"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Timer persistence is unavailable", body = ErrorResponse)
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

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Save Timer (pushes to provider via service layer)
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveActiveTimerRequest {
    /// Optional note override. When omitted, the active timer's note is used.
    note: Option<String>,
}

/// Save the active timer
///
/// Creates a provider time entry and finishes the local timer. `Idempotency-Key`
/// is required; retrying the same payload with the same key replays the original
/// result. A provider-neutral operation identifier is used for crash recovery.
#[utoipa::path(
    post,
    path = "/time-tracking/timer/save",
    operation_id = "saveActiveTimer",
    tag = "Time tracking",
    params(("Idempotency-Key" = String, Header, description = "Unique retry key for this save request")),
    request_body = SaveActiveTimerRequest,
    responses(
        (status = 201, description = "Timer saved", body = SaveTimerResponse),
        (status = 400, description = "Invalid timer or missing idempotency key", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "No active timer", body = ErrorResponse),
        (status = 409, description = "Idempotency conflict or locked period", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn save_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SaveActiveTimerRequest>,
) -> Result<(StatusCode, Json<SaveTimerResponse>), ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let note = body.note.map(normalize_user_note);
    let entry = service
        .save_timer(&user.id, note, idempotency_key(&headers)?)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(SaveTimerResponse {
            entry: TimeEntryResponse::from(entry),
        }),
    ))
}

// ============================================================================
// Edit Timer
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateActiveTimerRequest {
    /// Omit to preserve, set to `null` to clear, or provide an ID to select.
    #[serde(default, with = "serde_with::rust::double_option")]
    project_id: Option<Option<String>>,
    /// Omit to preserve, set to `null` to clear, or provide an ID to select.
    #[serde(default, with = "serde_with::rust::double_option")]
    activity_id: Option<Option<String>>,
    /// Optional note replacement.
    note: Option<String>,
    /// Optional RFC3339 start-time replacement.
    start_time: Option<String>,
}

/// Update the active timer
///
/// Applies a partial update and returns the canonical timer. Explicit `null`
/// clears a project or activity selection; clearing a project also clears its
/// activity.
#[utoipa::path(
    patch,
    path = "/time-tracking/timer",
    operation_id = "updateActiveTimer",
    tag = "Time tracking",
    request_body = UpdateActiveTimerRequest,
    responses(
        (status = 200, description = "Timer updated", body = TimerResponse),
        (status = 400, description = "Invalid patch or project/activity selection", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "No active timer", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn edit_timer(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<UpdateActiveTimerRequest>,
) -> Result<Json<TimerResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

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

    let request = UpdateTimerRequest {
        project_id: patch_id(body.project_id, ProjectId::new),
        activity_id: patch_id(body.activity_id, ActivityId::new),
        started_at: parsed_start_time,
        note: body.note.map(normalize_user_note),
    };
    let timer = service.edit_timer(&user.id, &request).await?;

    Ok(Json(timer.into()))
}

fn patch_id<T>(value: Option<Option<String>>, map: impl FnOnce(String) -> T) -> PatchValue<T> {
    match value {
        None => PatchValue::Unchanged,
        Some(None) => PatchValue::Clear,
        Some(Some(value)) => PatchValue::Set(map(value)),
    }
}

pub(super) fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    static IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
    headers
        .get(&IDEMPOTENCY_KEY)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("Idempotency-Key header is required"))
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
