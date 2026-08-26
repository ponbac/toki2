use axum::{
    extract::{Query, State},
    http::StatusCode,
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

use super::normalize_user_note;

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
#[serde(rename_all = "camelCase")]
pub struct EditProjectRegistrationPayload {
    project_registration_id: String,
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    start_time: String,
    end_time: String,
    user_note: String,
}

/// Update a time entry
///
/// Updates an existing time entry owned by the authenticated user.
#[utoipa::path(
    put,
    path = "/time-tracking/time-entries",
    operation_id = "updateTimeEntry",
    tag = "Time tracking",
    request_body = EditProjectRegistrationPayload,
    responses(
        (status = 200, description = "Time entry updated", body = TimeEntryResponse),
        (status = 400, description = "Invalid time entry", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "Time entry not found", body = ErrorResponse),
        (status = 409, description = "The time entry conflicts with provider state", body = ErrorResponse),
        (status = 500, description = "Time entry could not be updated", body = ErrorResponse)
    )
)]
pub async fn edit_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<EditProjectRegistrationPayload>,
) -> Result<Json<TimeEntryResponse>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = EditTimeEntryRequest {
        registration_id: payload.project_registration_id,
        project_id: ProjectId::new(payload.project_id),
        project_name: payload.project_name,
        activity_id: ActivityId::new(payload.activity_id),
        activity_name: payload.activity_name,
        start_time: parse_rfc3339(&payload.start_time, "start time")?,
        end_time: parse_rfc3339(&payload.end_time, "end time")?,
        note: normalize_user_note(payload.user_note),
    };

    let entry = service.edit_time_entry(&request).await?;

    Ok(Json(entry.into()))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectRegistrationPayload {
    project_registration_id: String,
}

/// Delete a time entry
///
/// Deletes an existing time entry owned by the authenticated user.
#[utoipa::path(
    delete,
    path = "/time-tracking/time-entries",
    operation_id = "deleteTimeEntry",
    tag = "Time tracking",
    request_body = DeleteProjectRegistrationPayload,
    responses(
        (status = 200, description = "Time entry deleted"),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "Time entry not found", body = ErrorResponse),
        (status = 409, description = "The time entry conflicts with provider state", body = ErrorResponse),
        (status = 500, description = "Time entry could not be deleted", body = ErrorResponse)
    )
)]
pub async fn delete_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<DeleteProjectRegistrationPayload>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    service
        .delete_time_entry(&payload.project_registration_id)
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRegistrationPayload {
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    start_time: String,
    end_time: String,
    user_note: String,
}

/// Create a time entry
///
/// Creates a time entry for the authenticated user without using the active
/// timer.
#[utoipa::path(
    post,
    path = "/time-tracking/time-entries",
    operation_id = "createTimeEntry",
    tag = "Time tracking",
    request_body = CreateProjectRegistrationPayload,
    responses(
        (status = 201, description = "Time entry created", body = TimeEntryResponse),
        (status = 400, description = "Invalid time entry", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "The time entry conflicts with provider state", body = ErrorResponse),
        (status = 500, description = "Time entry could not be created", body = ErrorResponse)
    )
)]
pub async fn create_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateProjectRegistrationPayload>,
) -> Result<(StatusCode, Json<TimeEntryResponse>), ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = CreateTimeEntryRequest {
        project_id: ProjectId::new(payload.project_id),
        project_name: payload.project_name,
        activity_id: ActivityId::new(payload.activity_id),
        activity_name: payload.activity_name,
        start_time: parse_rfc3339(&payload.start_time, "start time")?,
        end_time: parse_rfc3339(&payload.end_time, "end time")?,
        note: normalize_user_note(payload.user_note),
    };

    let entry = service.create_time_entry(&user.id, &request).await?;

    Ok((StatusCode::CREATED, Json(entry.into())))
}
