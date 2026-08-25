use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    adapters::inbound::http::{
        ErrorResponse, TimeEntryDayStatusResponse, TimeEntryResponse, WeeklyStatsResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::models::{ActivityId, CreateTimeEntryRequest, EditTimeEntryRequest, ProjectId},
    observability::record_user_id,
    routes::ApiError,
};

use super::{normalize_user_note, timer::idempotency_key};

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DateFilterQuery {
    /// Inclusive range start, `YYYY-MM-DD`.
    from: String,
    /// Inclusive range end, `YYYY-MM-DD`.
    to: String,
}

fn parse_date(s: &str) -> Result<time::Date, ApiError> {
    let format = time::format_description::parse("[year]-[month]-[day]").unwrap();
    time::Date::parse(s, &format)
        .map_err(|_| ApiError::bad_request(format!("could not parse date: {}", s)))
}

fn parse_rfc3339(s: &str, field: &str) -> Result<time::OffsetDateTime, ApiError> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map_err(|_| ApiError::bad_request(format!("Invalid {} format", field)))
}

/// Get worked and remaining hours
///
/// Returns weekly time statistics for an inclusive date range.
#[utoipa::path(
    get,
    path = "/time-tracking/time-info",
    operation_id = "getTimeInfo",
    tag = "Time tracking",
    params(DateFilterQuery),
    responses(
        (status = 200, description = "Worked, scheduled, and remaining hours", body = WeeklyStatsResponse),
        (status = 400, description = "Invalid date range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn get_time_info(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> Result<Json<WeeklyStatsResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let from = parse_date(&date_filter.from)?;
    let to = parse_date(&date_filter.to)?;

    let time_info = service.get_time_info((from, to)).await?;

    Ok(Json(time_info.into()))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TimeEntriesQuery {
    /// Inclusive range start, `YYYY-MM-DD`.
    from: String,
    /// Inclusive range end, `YYYY-MM-DD`.
    to: String,
    /// When true, collapse duplicate entries.
    unique: Option<bool>,
}

/// List time entries
///
/// Returns completed time registrations in an inclusive date range.
#[utoipa::path(
    get,
    path = "/time-tracking/time-entries",
    operation_id = "listTimeEntries",
    tag = "Time tracking",
    params(TimeEntriesQuery),
    responses(
        (status = 200, description = "Time entries in the requested range", body = Vec<TimeEntryResponse>),
        (status = 400, description = "Invalid date range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn get_time_entries(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> Result<Json<Vec<TimeEntryResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let from = parse_date(&query.from)?;
    let to = parse_date(&query.to)?;

    let time_entries = service
        .get_time_entries(&user.id, (from, to), query.unique.unwrap_or(false))
        .await?;

    Ok(Json(time_entries.into_iter().map(Into::into).collect()))
}

/// List time-entry day statuses
///
/// Returns attestation status for each day in an inclusive date range.
#[utoipa::path(
    get,
    path = "/time-tracking/time-entry-day-statuses",
    operation_id = "listTimeEntryDayStatuses",
    tag = "Time tracking",
    params(DateFilterQuery),
    responses(
        (status = 200, description = "Per-day attestation statuses", body = Vec<TimeEntryDayStatusResponse>),
        (status = 400, description = "Invalid date range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn get_time_entry_day_statuses(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> Result<Json<Vec<TimeEntryDayStatusResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let from = parse_date(&date_filter.from)?;
    let to = parse_date(&date_filter.to)?;

    let statuses = service.get_time_entry_day_statuses((from, to)).await?;

    Ok(Json(statuses.into_iter().map(Into::into).collect()))
}

// ============================================================================
// Time Entry Mutations (Create, Edit, Delete)
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTimeEntryRequest {
    project_id: String,
    activity_id: String,
    /// RFC3339 interval start.
    start_time: String,
    /// RFC3339 interval end.
    end_time: String,
    note: String,
}

/// Update a time entry
///
/// Updates an owned, open time entry using the registration ID in the path.
/// Project and activity names are resolved by Toki.
#[utoipa::path(
    put,
    path = "/time-tracking/time-entries/{registration_id}",
    operation_id = "updateTimeEntry",
    tag = "Time tracking",
    params(("registration_id" = String, Path, description = "Opaque provider registration ID")),
    request_body = UpdateTimeEntryRequest,
    responses(
        (status = 200, description = "Time entry updated", body = TimeEntryResponse),
        (status = 400, description = "Invalid interval or project/activity selection", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "Time entry not found or not owned by the user", body = ErrorResponse),
        (status = 409, description = "The time period is locked", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn update_time_entry(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(registration_id): Path<String>,
    Json(payload): Json<UpdateTimeEntryRequest>,
) -> Result<Json<TimeEntryResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = EditTimeEntryRequest {
        project_id: ProjectId::new(payload.project_id),
        activity_id: ActivityId::new(payload.activity_id),
        start_time: parse_rfc3339(&payload.start_time, "start time")?,
        end_time: parse_rfc3339(&payload.end_time, "end time")?,
        note: normalize_user_note(payload.note),
    };

    let entry = service.edit_time_entry(&registration_id, &request).await?;

    Ok(Json(entry.into()))
}

/// Delete a time entry
///
/// Permanently deletes an owned, open time entry. The registration ID is
/// supplied only in the path.
#[utoipa::path(
    delete,
    path = "/time-tracking/time-entries/{registration_id}",
    operation_id = "deleteTimeEntry",
    tag = "Time tracking",
    params(("registration_id" = String, Path, description = "Opaque provider registration ID")),
    responses(
        (status = 204, description = "Time entry deleted"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "Time entry not found or not owned by the user", body = ErrorResponse),
        (status = 409, description = "The time period is locked", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn delete_time_entry(
    user: AuthUser,
    State(app_state): State<AppState>,
    Path(registration_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    service.delete_time_entry(&registration_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTimeEntryRequestBody {
    project_id: String,
    activity_id: String,
    /// RFC3339 interval start.
    start_time: String,
    /// RFC3339 interval end.
    end_time: String,
    note: String,
}

/// Create a time entry
///
/// Creates a direct time entry without using the active timer. The required
/// `Idempotency-Key` makes successful retries replayable.
#[utoipa::path(
    post,
    path = "/time-tracking/time-entries",
    operation_id = "createTimeEntry",
    tag = "Time tracking",
    params(("Idempotency-Key" = String, Header, description = "Unique retry key for this create request")),
    request_body = CreateTimeEntryRequestBody,
    responses(
        (status = 201, description = "Time entry created", body = TimeEntryResponse),
        (status = 400, description = "Invalid interval, selection, or missing idempotency key", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "Idempotency conflict or locked period", body = ErrorResponse),
        (status = 503, description = "Time tracking provider is unavailable", body = ErrorResponse)
    )
)]
pub async fn create_time_entry(
    user: AuthUser,
    State(app_state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateTimeEntryRequestBody>,
) -> Result<(StatusCode, Json<TimeEntryResponse>), ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = CreateTimeEntryRequest {
        project_id: ProjectId::new(payload.project_id),
        activity_id: ActivityId::new(payload.activity_id),
        start_time: parse_rfc3339(&payload.start_time, "start time")?,
        end_time: parse_rfc3339(&payload.end_time, "end time")?,
        note: normalize_user_note(payload.note),
    };

    let entry = service
        .create_time_entry(&user.id, &request, idempotency_key(&headers)?)
        .await?;

    Ok((StatusCode::CREATED, Json(entry.into())))
}
