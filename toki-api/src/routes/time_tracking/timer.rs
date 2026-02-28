use crate::{
    adapters::inbound::http::{GetTimerResponse, TimerHistoryEntryResponse, TimerResponse},
    app_state::AppState,
    auth::AuthUser,
    domain::models::ActiveTimer,
    routes::ApiError,
};

use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::instrument;

use super::CookieJarResult;

const SAVE_TIMER_PARTIAL_UPDATE_ERROR: &str = "Project/activity update must be atomic: provide projectId, projectName, activityId, and activityName together.";

#[derive(Debug, PartialEq, Eq)]
struct AtomicSaveTimerProjectActivityUpdate {
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
}

fn parse_save_timer_project_activity_update(
    body: &SaveTimerPayload,
) -> Result<Option<AtomicSaveTimerProjectActivityUpdate>, ApiError> {
    let has_any_update = body.project_id.is_some()
        || body.project_name.is_some()
        || body.activity_id.is_some()
        || body.activity_name.is_some();

    if !has_any_update {
        return Ok(None);
    }

    match (
        body.project_id.clone(),
        body.project_name.clone(),
        body.activity_id.clone(),
        body.activity_name.clone(),
    ) {
        (Some(project_id), Some(project_name), Some(activity_id), Some(activity_name)) => {
            Ok(Some(AtomicSaveTimerProjectActivityUpdate {
                project_id,
                project_name,
                activity_id,
                activity_name,
            }))
        }
        _ => Err(ApiError::bad_request(SAVE_TIMER_PARTIAL_UPDATE_ERROR)),
    }
}

// ============================================================================
// Get Timer
// ============================================================================

#[instrument(name = "get_timer", skip(jar))]
pub async fn get_timer(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<GetTimerResponse>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let active_timer = service.get_active_timer(&user.id).await?;

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

#[instrument(name = "start_timer", skip(jar))]
pub async fn start_timer(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> CookieJarResult<StatusCode> {
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

    service.start_timer(&user.id, &timer).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Stop Timer
// ============================================================================

#[instrument(name = "stop_timer", skip(jar))]
pub async fn stop_timer(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    service.stop_timer(&user.id).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Save Timer (pushes to provider via service layer)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
}

#[instrument(name = "save_timer", skip(jar))]
pub async fn save_timer(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<SaveTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let parsed_update = parse_save_timer_project_activity_update(&body)?;
    let user_note = body.user_note;

    // Allow clients to include latest project/activity values in the save call.
    // This makes save robust if a prior timer-edit sync call was missed.
    if let Some(AtomicSaveTimerProjectActivityUpdate {
        project_id,
        project_name,
        activity_id,
        activity_name,
    }) = parsed_update
    {
        let current_timer = service
            .get_active_timer(&user.id)
            .await?
            .ok_or_else(|| ApiError::not_found("no active timer found"))?;

        let updated_timer = ActiveTimer::new(current_timer.started_at)
            .with_project(project_id, project_name)
            .with_activity(activity_id, activity_name)
            .with_note(current_timer.note);
        service.edit_timer(&user.id, &updated_timer).await?;
    }

    service.save_timer(&user.id, user_note).await?;

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

#[instrument(name = "edit_timer", skip(jar))]
pub async fn edit_timer(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<EditTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
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

    let note = body.user_note.unwrap_or(current_timer.note);
    updated_timer = updated_timer.with_note(note);

    service.edit_timer(&user.id, &updated_timer).await?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Timer History
// ============================================================================

#[instrument(name = "get_timer_history", skip(jar))]
pub async fn get_timer_history(
    jar: CookieJar,
    user: AuthUser,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<Vec<TimerHistoryEntryResponse>>> {
    let (service, jar) = app_state
        .time_tracking_factory
        .create_service(jar, &app_state.cookie_domain)
        .await?;

    let entries = service.get_timer_history(&user.id).await?;
    let response: Vec<TimerHistoryEntryResponse> = entries.into_iter().map(Into::into).collect();

    Ok((jar, Json(response)))
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    fn save_payload(
        project_id: Option<&str>,
        project_name: Option<&str>,
        activity_id: Option<&str>,
        activity_name: Option<&str>,
    ) -> SaveTimerPayload {
        SaveTimerPayload {
            user_note: None,
            project_id: project_id.map(ToString::to_string),
            project_name: project_name.map(ToString::to_string),
            activity_id: activity_id.map(ToString::to_string),
            activity_name: activity_name.map(ToString::to_string),
        }
    }

    #[test]
    fn parse_save_timer_update_accepts_no_project_or_activity_fields() {
        let body = save_payload(None, None, None, None);
        let parsed = if let Ok(parsed) = parse_save_timer_project_activity_update(&body) {
            parsed
        } else {
            panic!("expected parse success");
        };
        assert_eq!(parsed, None);
    }

    #[test]
    fn parse_save_timer_update_accepts_atomic_project_and_activity_fields() {
        let body = save_payload(Some("p1"), Some("Project"), Some("a1"), Some("Activity"));
        let parsed = if let Ok(parsed) = parse_save_timer_project_activity_update(&body) {
            parsed
        } else {
            panic!("expected parse success");
        };
        assert_eq!(
            parsed,
            Some(AtomicSaveTimerProjectActivityUpdate {
                project_id: "p1".to_string(),
                project_name: "Project".to_string(),
                activity_id: "a1".to_string(),
                activity_name: "Activity".to_string()
            })
        );
    }

    fn assert_bad_request_for_partial_update(body: SaveTimerPayload) {
        let err =
            parse_save_timer_project_activity_update(&body).expect_err("expected bad request");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn parse_save_timer_update_rejects_project_only_pair() {
        let body = save_payload(Some("p1"), Some("Project"), None, None);
        assert_bad_request_for_partial_update(body);
    }

    #[test]
    fn parse_save_timer_update_rejects_activity_only_pair() {
        let body = save_payload(None, None, Some("a1"), Some("Activity"));
        assert_bad_request_for_partial_update(body);
    }

    #[test]
    fn parse_save_timer_update_rejects_single_field_partials() {
        let cases = [
            save_payload(Some("p1"), None, None, None),
            save_payload(None, Some("Project"), None, None),
            save_payload(None, None, Some("a1"), None),
            save_payload(None, None, None, Some("Activity")),
        ];

        for body in cases {
            assert_bad_request_for_partial_update(body);
        }
    }

    #[test]
    fn parse_save_timer_update_rejects_mixed_partial_pairs() {
        let cases = [
            save_payload(Some("p1"), Some("Project"), Some("a1"), None),
            save_payload(Some("p1"), Some("Project"), None, Some("Activity")),
            save_payload(Some("p1"), None, Some("a1"), Some("Activity")),
            save_payload(None, Some("Project"), Some("a1"), Some("Activity")),
        ];

        for body in cases {
            assert_bad_request_for_partial_update(body);
        }
    }
}
