use crate::{
    adapters::inbound::http::{GetTimerResponse, TimerHistoryEntryResponse, TimerResponse},
    app_state::AppState,
    auth::AuthSession,
    domain::models::{ActiveTimer, UserId},
    routes::ApiError,
};

use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::instrument;

use super::CookieJarResult;

// ============================================================================
// Get Timer
// ============================================================================

#[instrument(name = "get_timer", skip(jar, app_state, auth_session))]
pub async fn get_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<GetTimerResponse>> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let active_timer = service.get_active_timer(&user_id).await?;

    let response = GetTimerResponse {
        timer: active_timer.map(TimerResponse::from),
    };

    Ok((jar, Json(response)))
}

// ============================================================================
// Start Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
}

#[instrument(name = "start_timer", skip(jar, app_state, auth_session))]
pub async fn start_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let mut timer = ActiveTimer::new(OffsetDateTime::now_utc());

    if let (Some(pid), Some(pname)) = (body.project_id, body.project_name) {
        timer = timer.with_project(pid, pname);
    }
    if let (Some(aid), Some(aname)) = (body.activity_id, body.activity_name) {
        timer = timer.with_activity(aid, aname);
    }
    if let Some(note) = body.user_note {
        timer = timer.with_note(note);
    }

    service.start_timer(&user_id, &timer).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Stop Timer
// ============================================================================

#[instrument(name = "stop_timer", skip(jar, app_state, auth_session))]
pub async fn stop_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    service.stop_timer(&user_id).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Save Timer (pushes to provider via service layer)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerPayload {
    user_note: Option<String>,
}

#[instrument(name = "save_timer", skip(app_state, auth_session, jar))]
pub async fn save_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<SaveTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    service.save_timer(&user_id, body.user_note).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Edit Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
    start_time: Option<String>,
}

#[instrument(name = "edit_timer", skip(jar, app_state, auth_session))]
pub async fn edit_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<EditTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    // Get the current timer to merge with edits
    let current_timer = service
        .get_active_timer(&user_id)
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

    let mut updated_timer =
        ActiveTimer::new(parsed_start_time.unwrap_or(current_timer.started_at));

    // Merge: use provided values or fall back to current timer
    if let (Some(pid), Some(pname)) = (
        body.project_id.or(current_timer.project_id.map(|p| p.to_string())),
        body.project_name.or(current_timer.project_name),
    ) {
        updated_timer = updated_timer.with_project(pid, pname);
    }

    if let (Some(aid), Some(aname)) = (
        body.activity_id.or(current_timer.activity_id.map(|a| a.to_string())),
        body.activity_name.or(current_timer.activity_name),
    ) {
        updated_timer = updated_timer.with_activity(aid, aname);
    }

    let note = body
        .user_note
        .unwrap_or(current_timer.note);
    updated_timer = updated_timer.with_note(note);

    service.edit_timer(&user_id, &updated_timer).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Timer History
// ============================================================================

#[instrument(name = "get_timer_history", skip(jar, auth_session, app_state))]
pub async fn get_timer_history(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<Vec<TimerHistoryEntryResponse>>> {
    let user = auth_session.user.expect("user not found");
    let user_id = UserId::from(user.id);

    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let entries = service.get_timer_history(&user_id).await?;
    let response: Vec<TimerHistoryEntryResponse> = entries.into_iter().map(Into::into).collect();

    Ok((jar, Json(response)))
}
