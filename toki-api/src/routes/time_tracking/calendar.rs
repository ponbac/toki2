use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{TimeEntryDayStatusResponse, TimeEntryResponse, WeeklyStatsResponse},
    app_state::AppState,
    auth::AuthUser,
    domain::models::{ActivityId, CreateTimeEntryRequest, EditTimeEntryRequest, ProjectId},
    routes::ApiError,
};

#[derive(Debug, Deserialize)]
pub struct DateFilterQuery {
    from: String,
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

#[instrument(name = "get_time_info", skip(app_state))]
pub async fn get_time_info(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> Result<Json<WeeklyStatsResponse>, ApiError> {
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let from = parse_date(&date_filter.from)?;
    let to = parse_date(&date_filter.to)?;

    let time_info = service.get_time_info((from, to)).await?;

    Ok(Json(time_info.into()))
}

#[derive(Debug, Deserialize)]
pub struct TimeEntriesQuery {
    from: String,
    to: String,
    unique: Option<bool>,
}

#[instrument(name = "get_time_entries", skip(app_state))]
pub async fn get_time_entries(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> Result<Json<Vec<TimeEntryResponse>>, ApiError> {
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

#[instrument(name = "get_time_entry_day_statuses", skip(app_state))]
pub async fn get_time_entry_day_statuses(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> Result<Json<Vec<TimeEntryDayStatusResponse>>, ApiError> {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditProjectRegistrationPayload {
    project_registration_id: String,
    project_id: String,
    activity_id: String,
    start_time: String,
    end_time: String,
    user_note: String,
}

#[instrument(name = "edit_project_registration", skip(app_state))]
pub async fn edit_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<EditProjectRegistrationPayload>,
) -> Result<StatusCode, ApiError> {
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = EditTimeEntryRequest {
        registration_id: payload.project_registration_id,
        project_id: ProjectId::new(payload.project_id),
        activity_id: ActivityId::new(payload.activity_id),
        start_time: parse_rfc3339(&payload.start_time, "start time")?,
        end_time: parse_rfc3339(&payload.end_time, "end time")?,
        note: payload.user_note,
    };

    service.edit_time_entry(&request).await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectRegistrationPayload {
    project_registration_id: String,
}

#[instrument(name = "delete_project_registration", skip(app_state))]
pub async fn delete_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<DeleteProjectRegistrationPayload>,
) -> Result<StatusCode, ApiError> {
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    service
        .delete_time_entry(&payload.project_registration_id)
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
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

#[instrument(name = "create_project_registration", skip(app_state))]
pub async fn create_project_registration(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateProjectRegistrationPayload>,
) -> Result<StatusCode, ApiError> {
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
        note: payload.user_note,
    };

    service.create_time_entry(&user.id, &request).await?;

    Ok(StatusCode::CREATED)
}
