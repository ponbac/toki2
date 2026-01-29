use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{TimeEntryResponse, TimeInfoResponse, TimeTrackingServiceExt},
    app_state::AppState,
    auth::AuthSession,
    domain::{
        models::{
            ActivityId, CreateTimeEntryRequest, EditTimeEntryRequest, ProjectId, UserId,
        },
        ports::inbound::TimeTrackingService,
    },
};

use super::{CookieJarResult, ErrorResponse, MilltimeError};

#[derive(Debug, Deserialize)]
pub struct DateFilterQuery {
    from: String,
    to: String,
}

fn parse_date(s: &str) -> Result<time::Date, ErrorResponse> {
    let format = time::format_description::parse("[year]-[month]-[day]").unwrap();
    time::Date::parse(s, &format).map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: format!("could not parse date: {}", s),
    })
}

#[instrument(name = "get_time_info", skip(jar, app_state))]
pub async fn get_time_info(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> CookieJarResult<Json<TimeInfoResponse>> {
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    let from = parse_date(&date_filter.from)?;
    let to = parse_date(&date_filter.to)?;

    let time_info = service
        .get_time_info((from, to))
        .await
        .map_err(tracking_error_to_response)?;

    Ok((jar, Json(time_info.into())))
}

#[derive(Debug, Deserialize)]
pub struct TimeEntriesQuery {
    from: String,
    to: String,
    unique: Option<bool>,
}

#[instrument(name = "get_time_entries", skip(jar, app_state, auth_session))]
pub async fn get_time_entries(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> CookieJarResult<Json<Vec<TimeEntryResponse>>> {
    let (service, jar) = jar
        .into_time_tracking_service_with_history(
            &app_state.cookie_domain,
            app_state.milltime_repo.clone(),
        )
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let from = parse_date(&query.from)?;
    let to = parse_date(&query.to)?;

    let time_entries = service
        .get_time_entries(&user_id, (from, to), query.unique.unwrap_or(false))
        .await
        .map_err(tracking_error_to_response)?;

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
}

#[instrument(name = "edit_project_registration", skip(jar, app_state))]
pub async fn edit_project_registration(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Json(payload): Json<EditProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = jar
        .into_time_tracking_service_with_history(
            &app_state.cookie_domain,
            app_state.milltime_repo.clone(),
        )
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    let start_time = time::OffsetDateTime::parse(
        &payload.start_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid start time format".to_string(),
    })?;

    let end_time = time::OffsetDateTime::parse(
        &payload.end_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid end time format".to_string(),
    })?;

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
    };

    service
        .edit_time_entry(&request)
        .await
        .map_err(tracking_error_to_response)?;

    Ok((jar, StatusCode::OK))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectRegistrationPayload {
    project_registration_id: String,
}

#[instrument(name = "delete_project_registration", skip(jar, app_state))]
pub async fn delete_project_registration(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Json(payload): Json<DeleteProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    service
        .delete_time_entry(&payload.project_registration_id)
        .await
        .map_err(tracking_error_to_response)?;

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

#[instrument(
    name = "create_project_registration",
    skip(jar, app_state, auth_session)
)]
pub async fn create_project_registration(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = jar
        .into_time_tracking_service_with_history(
            &app_state.cookie_domain,
            app_state.milltime_repo.clone(),
        )
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let start_time = time::OffsetDateTime::parse(
        &payload.start_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid start time format".to_string(),
    })?;

    let end_time = time::OffsetDateTime::parse(
        &payload.end_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid end time format".to_string(),
    })?;

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

    service
        .create_time_entry(&user_id, &request)
        .await
        .map_err(tracking_error_to_response)?;

    Ok((jar, StatusCode::CREATED))
}

fn tracking_error_to_response(e: crate::domain::TimeTrackingError) -> ErrorResponse {
    use crate::domain::TimeTrackingError;

    match e {
        TimeTrackingError::AuthenticationFailed => ErrorResponse {
            status: StatusCode::UNAUTHORIZED,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: "Authentication failed".to_string(),
        },
        TimeTrackingError::InvalidDateRange => ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            error: MilltimeError::DateParseError,
            message: "Invalid date range".to_string(),
        },
        _ => ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: e.to_string(),
        },
    }
}
