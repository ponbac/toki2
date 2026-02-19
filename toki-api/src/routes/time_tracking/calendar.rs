use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{TimeEntryResponse, TimeInfoResponse},
    app_state::AppState,
    auth::AuthUser,
    domain::models::{ActivityId, CreateTimeEntryRequest, EditTimeEntryRequest, ProjectId},
    routes::ApiError,
};

use super::CookieJarResult;

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

#[instrument(name = "get_time_info", skip(jar))]
pub async fn get_time_info(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> CookieJarResult<Json<TimeInfoResponse>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let from = parse_date(&date_filter.from)?;
    let to = parse_date(&date_filter.to)?;

    let time_info = service.get_time_info((from, to)).await?;

    Ok((jar, Json(time_info.into())))
}

#[derive(Debug, Deserialize)]
pub struct TimeEntriesQuery {
    from: String,
    to: String,
    unique: Option<bool>,
}

#[instrument(name = "get_time_entries", skip(jar))]
pub async fn get_time_entries(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> CookieJarResult<Json<Vec<TimeEntryResponse>>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let from = parse_date(&query.from)?;
    let to = parse_date(&query.to)?;

    let time_entries = service
        .get_time_entries(&user.id, (from, to), query.unique.unwrap_or(false))
        .await?;

    let response: Vec<TimeEntryResponse> = time_entries.into_iter().map(Into::into).collect();

    Ok((jar, Json(response)))
}

// ============================================================================
// Time Entry Mutations (Create, Edit, Delete)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditProjectRegistrationPayload {
    project_registration_id: String,
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    start_time: String,
    end_time: String,
    reg_day: String,
    week_number: i32,
    user_note: String,
    original_reg_day: Option<String>,
    original_project_id: Option<String>,
    original_activity_id: Option<String>,
}

#[instrument(name = "edit_project_registration", skip(jar))]
pub async fn edit_project_registration(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Json(payload): Json<EditProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let start_time = time::OffsetDateTime::parse(
        &payload.start_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ApiError::bad_request("Invalid start time format"))?;

    let end_time = time::OffsetDateTime::parse(
        &payload.end_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ApiError::bad_request("Invalid end time format"))?;

    let request = EditTimeEntryRequest {
        registration_id: payload.project_registration_id,
        project_id: ProjectId::new(payload.project_id),
        project_name: payload.project_name,
        activity_id: ActivityId::new(payload.activity_id),
        activity_name: payload.activity_name,
        start_time,
        end_time,
        reg_day: payload.reg_day,
        week_number: payload.week_number,
        note: payload.user_note,
        original_reg_day: payload.original_reg_day,
        original_project_id: payload.original_project_id.map(ProjectId::new),
        original_activity_id: payload.original_activity_id.map(ActivityId::new),
    };

    service.edit_time_entry(&request).await?;

    Ok((jar, StatusCode::OK))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectRegistrationPayload {
    project_registration_id: String,
}

#[instrument(name = "delete_project_registration", skip(jar))]
pub async fn delete_project_registration(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Json(payload): Json<DeleteProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    service
        .delete_time_entry(&payload.project_registration_id)
        .await?;

    Ok((jar, StatusCode::OK))
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
    reg_day: String,
    week_number: i32,
    user_note: String,
}

#[instrument(name = "create_project_registration", skip(jar))]
pub async fn create_project_registration(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let start_time = time::OffsetDateTime::parse(
        &payload.start_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ApiError::bad_request("Invalid start time format"))?;

    let end_time = time::OffsetDateTime::parse(
        &payload.end_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ApiError::bad_request("Invalid end time format"))?;

    let request = CreateTimeEntryRequest {
        project_id: ProjectId::new(payload.project_id),
        project_name: payload.project_name,
        activity_id: ActivityId::new(payload.activity_id),
        activity_name: payload.activity_name,
        start_time,
        end_time,
        reg_day: payload.reg_day,
        week_number: payload.week_number,
        note: payload.user_note,
    };

    service.create_time_entry(&user.id, &request).await?;

    Ok((jar, StatusCode::CREATED))
}
