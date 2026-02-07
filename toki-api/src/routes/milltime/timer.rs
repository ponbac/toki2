use crate::{
    adapters::inbound::http::{
        GetTimerResponse, TimeTrackingServiceError, TimeTrackingServiceExt, TimerResponse,
    },
    app_state::AppState,
    auth::AuthSession,
    domain::{
        models::CreateTimeEntryRequest,
        ports::inbound::TimeTrackingService,
    },
    repositories::{self, DatabaseTimer, TimerRepository},
};

use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::instrument;

use super::{CookieJarResult, ErrorResponse, MilltimeError};

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

    // Try to get timer from Milltime via service
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(|e| ErrorResponse {
            status: e.status,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: e.message,
        })?;

    let mt_timer = service.get_active_timer().await;

    // Also check database for active timer
    let db_timer = app_state.timer_repo.active_timer(&user.id).await;

    match (mt_timer, db_timer) {
        // Milltime timer exists - use it
        (Ok(Some(timer)), _) => Ok((
            jar,
            Json(GetTimerResponse {
                timer: Some(timer.into()),
            }),
        )),
        // Database timer exists - use it
        (Ok(None), Ok(Some(db_timer))) => Ok((
            jar,
            Json(GetTimerResponse {
                timer: Some(database_timer_to_response(&db_timer)),
            }),
        )),
        // Milltime fetch failed but we have a database timer - use it
        (Err(e), Ok(Some(db_timer))) => {
            tracing::warn!("failed to fetch milltime timer, but found timer in db: {:?}", e);
            Ok((
                jar,
                Json(GetTimerResponse {
                    timer: Some(database_timer_to_response(&db_timer)),
                }),
            ))
        }
        // No timer found anywhere
        _ => Ok((jar, Json(GetTimerResponse { timer: None }))),
    }
}

fn database_timer_to_response(timer: &DatabaseTimer) -> TimerResponse {
    let elapsed = OffsetDateTime::now_utc() - timer.start_time;
    let total_seconds = elapsed.whole_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    TimerResponse {
        start_time: timer.start_time,
        project_id: timer.project_id.clone(),
        project_name: timer.project_name.clone(),
        activity_id: timer.activity_id.clone(),
        activity_name: timer.activity_name.clone(),
        note: timer.note.clone().unwrap_or_default(),
        hours,
        minutes,
        seconds,
    }
}

// ============================================================================
// Start Timer (DB-based, no provider call)
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

#[instrument(name = "start_timer", skip(app_state, auth_session))]
pub async fn start_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    if let Ok(Some(_)) = app_state.timer_repo.active_timer(&user.id).await {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            error: MilltimeError::TimerError,
            message: "user already has an active timer".to_string(),
        });
    }

    let new_timer = repositories::NewDatabaseTimer {
        user_id: user.id,
        start_time: OffsetDateTime::now_utc(),
        project_id: body.project_id,
        project_name: body.project_name,
        activity_id: body.activity_id,
        activity_name: body.activity_name,
        note: body.user_note.unwrap_or_default(),
    };

    match app_state.timer_repo.create_timer(&new_timer).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            tracing::error!("failed to create timer: {:?}", e);
            Err(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                error: MilltimeError::DatabaseError,
                message: "failed to save timer to db".to_string(),
            })
        }
    }
}

// ============================================================================
// Stop Timer (DB-based, no provider call)
// ============================================================================

#[instrument(name = "stop_timer", skip(app_state, auth_session))]
pub async fn stop_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    if let Err(e) = app_state.timer_repo.delete_active_timer(&user.id).await {
        tracing::error!("failed to delete active timer: {:?}", e);
        return Err(ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::DatabaseError,
            message: "failed to delete active timer".to_string(),
        });
    }

    Ok(StatusCode::OK)
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
    let active_timer = match app_state.timer_repo.active_timer(&user.id).await {
        Ok(Some(timer)) => timer,
        _ => {
            return Err(ErrorResponse {
                status: StatusCode::NOT_FOUND,
                error: MilltimeError::TimerError,
                message: "no active timer found".to_string(),
            })
        }
    };

    // Create service without history to avoid duplicate DB entry
    // (the active timer row already exists and will be updated below)
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    const BONUS_TIME_MINUTES: i64 = 1;
    let now = OffsetDateTime::now_utc();
    let end_time = now + time::Duration::minutes(BONUS_TIME_MINUTES);

    let current_day = now
        .date()
        .format(&time::format_description::parse("[year]-[month]-[day]").unwrap())
        .expect("failed to format current day");
    let week_number = now.iso_week() as i32;

    let req = CreateTimeEntryRequest {
        project_id: active_timer.project_id.expect("project id not found").into(),
        project_name: active_timer.project_name.expect("project name not found"),
        activity_id: active_timer.activity_id.expect("activity id not found").into(),
        activity_name: active_timer.activity_name.expect("activity name not found"),
        start_time: active_timer.start_time,
        end_time,
        reg_day: current_day,
        week_number,
        note: body.user_note.unwrap_or(active_timer.note.unwrap_or_default()),
    };

    let timer_id = service
        .create_time_entry(&user.id.into(), &req)
        .await
        .map_err(tracking_error_to_response)?;

    // Update local DB: mark the active timer as saved with the provider's registration ID
    let end_time_db = OffsetDateTime::now_utc();
    app_state
        .timer_repo
        .save_active_timer(&user.id, &end_time_db, timer_id.as_str())
        .await
        .map_err(|e| ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::DatabaseError,
            message: format!("failed to save active timer to db: {:?}", e),
        })?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Edit Timer (DB-based, no provider call)
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

#[instrument(name = "edit_timer", skip(app_state, auth_session))]
pub async fn edit_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<EditTimerPayload>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    let active_timer = match app_state.timer_repo.active_timer(&user.id).await {
        Ok(Some(timer)) => timer,
        _ => {
            return Err(ErrorResponse {
                status: StatusCode::NOT_FOUND,
                error: MilltimeError::TimerError,
                message: "no active timer found".to_string(),
            });
        }
    };

    let parsed_start_time_option: Option<OffsetDateTime> = body
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
                    ErrorResponse {
                        status: StatusCode::BAD_REQUEST,
                        error: MilltimeError::TimerError,
                        message: format!(
                            "Invalid start_time format. Expected ISO 8601 string. Details: {}",
                            e
                        ),
                    }
                })
        })
        .transpose()?;

    let update_timer_data = repositories::UpdateDatabaseTimer {
        user_id: user.id,
        user_note: body
            .user_note
            .unwrap_or_else(|| active_timer.note.clone().unwrap_or_default()),
        project_id: body.project_id.or(active_timer.project_id),
        project_name: body.project_name.or(active_timer.project_name),
        activity_id: body.activity_id.or(active_timer.activity_id),
        activity_name: body.activity_name.or(active_timer.activity_name),
        start_time: parsed_start_time_option,
    };

    if let Err(e) = app_state
        .timer_repo
        .update_timer(&update_timer_data)
        .await
    {
        tracing::error!("failed to update timer: {:?}", e);
        return Err(ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::DatabaseError,
            message: "failed to update timer".to_string(),
        });
    }

    Ok(StatusCode::OK)
}

// ============================================================================
// Timer History
// ============================================================================

#[instrument(name = "get_timer_history", skip(auth_session, app_state))]
pub async fn get_timer_history(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<DatabaseTimer>>, (StatusCode, String)> {
    let user = auth_session.user.expect("user not found");
    let timers = app_state.timer_repo.get_timer_history(&user.id).await;

    match timers {
        Ok(timers) => Ok(Json(timers)),
        Err(e) => {
            tracing::error!("failed to fetch timer history: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "".to_string()))
        }
    }
}

// ============================================================================
// Error Mapping
// ============================================================================

fn service_error_to_response(e: TimeTrackingServiceError) -> ErrorResponse {
    ErrorResponse {
        status: e.status,
        error: MilltimeError::MilltimeAuthenticationFailed,
        message: e.message,
    }
}

fn tracking_error_to_response(e: crate::domain::TimeTrackingError) -> ErrorResponse {
    use crate::domain::TimeTrackingError;

    match e {
        TimeTrackingError::AuthenticationFailed => ErrorResponse {
            status: StatusCode::UNAUTHORIZED,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: "Authentication failed".to_string(),
        },
        TimeTrackingError::TimerNotFound | TimeTrackingError::NoTimerRunning => ErrorResponse {
            status: StatusCode::NOT_FOUND,
            error: MilltimeError::TimerError,
            message: e.to_string(),
        },
        TimeTrackingError::TimerAlreadyRunning => ErrorResponse {
            status: StatusCode::CONFLICT,
            error: MilltimeError::TimerError,
            message: e.to_string(),
        },
        _ => ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: e.to_string(),
        },
    }
}
