use crate::{
    adapters::inbound::http::{
        GetTimerResponse, TimeTrackingServiceError, TimeTrackingServiceExt, TimerResponse,
    },
    app_state::AppState,
    auth::AuthSession,
    domain::{
        models::StartTimerRequest,
        ports::inbound::TimeTrackingService,
    },
    repositories::{self, DatabaseTimer, TimerRepository},
};

use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use time::OffsetDateTime;
use tracing::instrument;

use super::{CookieJarResult, ErrorResponse, MilltimeCookieJarExt, MilltimeError};

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
    let db_timer = app_state.milltime_repo.active_timer(&user.id).await;

    match (mt_timer, db_timer) {
        // Milltime timer exists - use it (displayed as Standalone now)
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
// Start Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerPayload {
    activity: String,
    activity_name: String,
    project_id: String,
    project_name: String,
    user_note: Option<String>,
    reg_day: String,
    week_number: i64,
}

#[instrument(name = "start_timer", skip(jar, app_state, auth_session))]
pub async fn start_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");

    // Check if user has active timer
    if let Ok(Some(_)) = app_state.milltime_repo.active_timer(&user.id).await {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            error: MilltimeError::TimerError,
            message: "user already has an active timer".to_string(),
        });
    }

    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    let req = StartTimerRequest::new(
        body.project_id.clone(),
        body.project_name.clone(),
        body.activity.clone(),
        body.activity_name.clone(),
        body.reg_day.clone(),
        body.week_number,
    );
    let req = match body.user_note.as_ref() {
        Some(note) => req.with_note(note),
        None => req,
    };

    service
        .start_timer(&req)
        .await
        .map_err(tracking_error_to_response)?;

    // Save to local database
    let new_timer = repositories::NewDatabaseTimer {
        user_id: user.id,
        start_time: OffsetDateTime::now_utc(),
        project_id: Some(body.project_id),
        project_name: Some(body.project_name),
        activity_id: Some(body.activity),
        activity_name: Some(body.activity_name),
        note: body.user_note.unwrap_or_default(),
    };

    if let Err(e) = app_state.milltime_repo.create_timer(&new_timer).await {
        tracing::error!("failed to create timer in db: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Start Standalone Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartStandaloneTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
}

#[instrument(name = "start_standalone_timer", skip(app_state, auth_session))]
pub async fn start_standalone_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartStandaloneTimerPayload>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    if let Ok(Some(_)) = app_state.milltime_repo.active_timer(&user.id).await {
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

    match app_state.milltime_repo.create_timer(&new_timer).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            tracing::error!("failed to create standalone timer: {:?}", e);
            Err(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                error: MilltimeError::DatabaseError,
                message: "failed to save standalone timer to db".to_string(),
            })
        }
    }
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
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    service
        .stop_timer()
        .await
        .map_err(tracking_error_to_response)?;

    let user = auth_session.user.expect("user not found");
    if let Err(e) = app_state.milltime_repo.delete_active_timer(&user.id).await {
        tracing::error!("failed to delete active timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[instrument(name = "stop_standalone_timer", skip(app_state, auth_session))]
pub async fn stop_standalone_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    if let Err(e) = app_state.milltime_repo.delete_active_timer(&user.id).await {
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
// Save Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerPayload {
    user_note: Option<String>,
}

#[instrument(name = "save_timer", skip(jar, app_state, auth_session))]
pub async fn save_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<SaveTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (service, jar) = jar
        .into_time_tracking_service(&app_state.cookie_domain)
        .await
        .map_err(service_error_to_response)?;

    let registration_id = service
        .save_timer(body.user_note.as_deref())
        .await
        .map_err(tracking_error_to_response)?;

    let user = auth_session.user.expect("user not found");
    let end_time = OffsetDateTime::now_utc();
    if let Err(e) = app_state
        .milltime_repo
        .save_active_timer(&user.id, &end_time, registration_id.as_str())
        .await
    {
        tracing::error!("failed to save active timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[instrument(name = "save_standalone_timer", skip(app_state, auth_session, jar))]
pub async fn save_standalone_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<SaveTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let user = auth_session.user.expect("user not found");
    let active_timer = match app_state.milltime_repo.active_timer(&user.id).await {
        Ok(Some(timer)) => timer,
        _ => {
            return Err(ErrorResponse {
                status: StatusCode::NOT_FOUND,
                error: MilltimeError::TimerError,
                message: "no active timer found".to_string(),
            })
        }
    };

    // Get Milltime client for saving to Milltime
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    const BONUS_TIME_MINUTES: i64 = 1;
    let total_time = {
        let duration = OffsetDateTime::now_utc() - active_timer.start_time;
        let total_minutes = duration.whole_minutes();
        let hours = total_minutes / 60;
        let minutes = (total_minutes % 60) + BONUS_TIME_MINUTES;
        format!("{:02}:{:02}", hours, minutes)
    };

    let current_day = OffsetDateTime::now_utc()
        .date()
        .format(&time::format_description::parse("[year]-[month]-[day]").unwrap())
        .expect("failed to format current day");
    let week_number = OffsetDateTime::now_utc().iso_week();

    let payload = milltime::ProjectRegistrationPayload::new(
        milltime_client.user_id().to_string(),
        active_timer.project_id.expect("project id not found"),
        active_timer.project_name.expect("project name not found"),
        active_timer.activity_id.expect("activity id not found"),
        active_timer.activity_name.expect("activity name not found"),
        total_time,
        current_day.to_string(),
        week_number.into(),
        body.user_note
            .unwrap_or(active_timer.note.unwrap_or_default()),
    );

    let registration = milltime_client.new_project_registration(&payload).await?;

    let end_time = OffsetDateTime::now_utc();
    app_state
        .milltime_repo
        .save_active_timer(&user.id, &end_time, &registration.project_registration_id)
        .await
        .map_err(|e| ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::DatabaseError,
            message: format!("failed to save active timer to db: {:?}", e),
        })?;

    Ok((jar, StatusCode::OK))
}

// ============================================================================
// Edit Timer
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTimerPayload {
    user_note: String,
}

#[instrument(name = "edit_timer", skip(jar, app_state, auth_session))]
pub async fn edit_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<EditTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let payload = milltime::EditTimerPayload {
        user_note: body.user_note.clone(),
    };
    milltime_client.edit_timer(&payload).await?;

    let user = auth_session.user.expect("user not found");
    let update_timer = repositories::UpdateDatabaseTimer {
        user_id: user.id,
        user_note: body.user_note,
        project_id: None,
        project_name: None,
        activity_id: None,
        activity_name: None,
        start_time: None,
    };
    if let Err(e) = app_state.milltime_repo.update_timer(&update_timer).await {
        tracing::error!("failed to update timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditStandaloneTimerPayload {
    user_note: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
    start_time: Option<String>,
}

#[instrument(name = "edit_standalone_timer", skip(app_state, auth_session))]
pub async fn edit_standalone_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<EditStandaloneTimerPayload>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    let active_timer = match app_state.milltime_repo.active_timer(&user.id).await {
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
        .milltime_repo
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
    let timers = app_state.milltime_repo.get_timer_history(&user.id).await;

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
