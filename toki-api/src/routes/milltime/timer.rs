use crate::{
    repositories::{TimerRepository, TimerType},
    routes::milltime::{ErrorResponse, MilltimeError},
};

use axum::{debug_handler, extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use milltime::MilltimeFetchError;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{app_state::AppState, auth::AuthSession, repositories};

use super::{CookieJarResult, MilltimeCookieJarExt};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MilltimeTimerWrapper {
    #[serde(flatten)]
    timer: milltime::TimerRegistration,
    timer_type: TimerType,
}

impl From<milltime::TimerRegistration> for MilltimeTimerWrapper {
    fn from(timer: milltime::TimerRegistration) -> Self {
        Self {
            timer,
            timer_type: TimerType::Milltime,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TokiTimer {
    Milltime(MilltimeTimerWrapper),
    Standalone(repositories::DatabaseTimer),
}

#[debug_handler]
#[instrument(name = "get_timer", skip(jar, app_state, auth_session))]
pub async fn get_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<TokiTimer>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let mt_timer = milltime_client.fetch_timer().await;

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let db_timer = milltime_repo.active_timer(&user.id).await;

    match (mt_timer, db_timer) {
        (Ok(mt_timer), Ok(Some(_))) => Ok((jar, Json(TokiTimer::Milltime(mt_timer.into())))),
        (Ok(mt_timer), Ok(None)) => {
            tracing::warn!("milltime timer found but no active timer in db");
            Ok((jar, Json(TokiTimer::Milltime(mt_timer.into()))))
        }
        (Ok(mt_timer), Err(e)) => {
            tracing::warn!("failed to fetch single active timer in db: {:?}", e);
            Ok((jar, Json(TokiTimer::Milltime(mt_timer.into()))))
        }
        (Err(e), Ok(Some(db_timer))) => {
            tracing::warn!("failed to fetch milltime timer, but found in db: {:?}", e);
            if db_timer.timer_type == TimerType::Standalone {
                Ok((jar, Json(TokiTimer::Standalone(db_timer))))
            } else {
                match e {
                    MilltimeFetchError::ResponseError(_) => {
                        tracing::warn!("response error, not deleting active timer");
                    }
                    _ => milltime_repo.delete_active_timer(&user.id).await.unwrap(),
                }
                Err(ErrorResponse {
                    status: StatusCode::NOT_FOUND,
                    error: MilltimeError::TimerError,
                    message: "non-standalone timer found in db but not on milltime".to_string(),
                })
            }
        }
        _ => Err(ErrorResponse {
            status: StatusCode::NOT_FOUND,
            error: MilltimeError::TimerError,
            message: "timer not found in db or on milltime".to_string(),
        }),
    }
}

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
    input_time: Option<String>,
    proj_time: Option<String>,
}

#[instrument(name = "start_timer", skip(jar, app_state, auth_session))]
pub async fn start_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> CookieJarResult<StatusCode> {
    // check if user has active timer, if so, return error
    let user = auth_session.user.expect("user not found");
    if let Ok(Some(_)) = app_state.milltime_repo.active_timer(&user.id).await {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            error: MilltimeError::TimerError,
            message: "user already has an active timer".to_string(),
        });
    }

    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let start_timer_options = milltime::StartTimerOptions::new(
        body.activity.clone(),
        body.activity_name.clone(),
        body.project_id.clone(),
        body.project_name.clone(),
        milltime_client.user_id().to_string(),
        body.user_note.clone(),
        body.reg_day.clone(),
        body.week_number,
        body.input_time,
        body.proj_time,
    );

    milltime_client.start_timer(start_timer_options).await?;

    let new_timer = repositories::NewDatabaseTimer {
        user_id: user.id,
        start_time: time::OffsetDateTime::now_utc(),
        project_id: Some(body.project_id.clone()),
        project_name: Some(body.project_name.clone()),
        activity_id: Some(body.activity.clone()),
        activity_name: Some(body.activity_name.clone()),
        note: body.user_note.clone().unwrap_or_default(),
        timer_type: TimerType::Milltime,
    };

    if let Err(e) = app_state.milltime_repo.create_timer(&new_timer).await {
        tracing::error!("failed to create timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartStandaloneTimerPayload {
    user_note: Option<String>,
}

#[instrument(name = "start_standalone_timer", skip(app_state, auth_session))]
pub async fn start_standalone_timer(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartStandaloneTimerPayload>,
) -> Result<StatusCode, ErrorResponse> {
    let user = auth_session.user.expect("user not found");

    // check if user has active timer, if so, return error
    if let Ok(Some(_)) = app_state.milltime_repo.active_timer(&user.id).await {
        return Err(ErrorResponse {
            status: StatusCode::CONFLICT,
            error: MilltimeError::TimerError,
            message: "user already has an active timer".to_string(),
        });
    }

    let new_timer = repositories::NewDatabaseTimer {
        user_id: user.id,
        start_time: time::OffsetDateTime::now_utc(),
        project_id: None,
        project_name: None,
        activity_id: None,
        activity_name: None,
        note: body.user_note.clone().unwrap_or_default(),
        timer_type: TimerType::Standalone,
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

#[instrument(name = "stop_timer", skip(jar, app_state, auth_session))]
pub async fn stop_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    milltime_client.stop_timer().await?;

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

#[instrument(name = "save_timer", skip(jar, app_state, auth_session))]
#[debug_handler]
pub async fn save_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<milltime::SaveTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let registration = milltime_client.save_timer(body).await?;

    let user = auth_session.user.expect("user not found");
    let end_time = time::OffsetDateTime::now_utc();
    if let Err(e) = app_state
        .milltime_repo
        .save_active_timer(&user.id, &end_time, &registration.project_registration_id)
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
    Json(body): Json<milltime::SaveTimerPayload>,
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

    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let total_time = {
        let duration = time::OffsetDateTime::now_utc() - active_timer.start_time;
        let total_minutes = duration.whole_minutes();
        let hours = total_minutes / 60;
        let minutes = (total_minutes % 60) + 1; // maybe should use rounding instead of adding 1
        format!("{:02}:{:02}", hours, minutes)
    };
    let current_day = time::OffsetDateTime::now_utc()
        .date()
        .format(&time::format_description::parse("[year]-[month]-[day]").unwrap())
        .expect("failed to format current day");
    let week_number = time::OffsetDateTime::now_utc().iso_week();

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

    let end_time = time::OffsetDateTime::now_utc();
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

#[instrument(name = "edit_timer", skip(jar, app_state, auth_session))]
pub async fn edit_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<milltime::EditTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    milltime_client.edit_timer(&body).await?;

    let user = auth_session.user.expect("user not found");
    let update_timer = repositories::UpdateDatabaseTimer {
        user_id: user.id,
        user_note: body.user_note,
        project_id: None,
        project_name: None,
        activity_id: None,
        activity_name: None,
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

    let update_timer = repositories::UpdateDatabaseTimer {
        user_id: user.id,
        user_note: body.user_note.unwrap_or_default(),
        project_id: body.project_id.or(active_timer.project_id),
        project_name: body.project_name.or(active_timer.project_name),
        activity_id: body.activity_id.or(active_timer.activity_id),
        activity_name: body.activity_name.or(active_timer.activity_name),
    };

    if let Err(e) = app_state.milltime_repo.update_timer(&update_timer).await {
        tracing::error!("failed to update timer: {:?}", e);
        return Err(ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::DatabaseError,
            message: "failed to update timer".to_string(),
        });
    }

    Ok(StatusCode::OK)
}

#[instrument(name = "get_timer_history", skip(auth_session, app_state))]
pub async fn get_timer_history(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<repositories::DatabaseTimer>>, (StatusCode, String)> {
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
