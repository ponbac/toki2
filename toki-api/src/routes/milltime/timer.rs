use crate::{
    repositories::MilltimeRepository,
    routes::milltime::{ErrorResponse, MilltimeError},
};

use axum::{debug_handler, extract::State, http::StatusCode, Json};
use axum_extra::extract::CookieJar;
use milltime::MilltimeFetchError;
use serde::Deserialize;
use tracing::instrument;

use crate::{app_state::AppState, auth::AuthSession, repositories};

use super::{CookieJarResult, MilltimeCookieJarExt};

#[instrument(name = "get_timer", skip(jar, app_state, auth_session))]
pub async fn get_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<milltime::TimerRegistration>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let mt_timer = milltime_client.fetch_timer().await;

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let db_timer = milltime_repo.active_timer(&user.id).await;

    match (mt_timer, db_timer) {
        (Ok(mt_timer), Ok(Some(_))) => Ok((jar, Json(mt_timer))),
        (Ok(mt_timer), Ok(None)) => {
            tracing::warn!("milltime timer found but no active timer in db");
            Ok((jar, Json(mt_timer)))
        }
        (Ok(mt_timer), Err(e)) => {
            tracing::warn!("failed to fetch single active timer in db: {:?}", e);
            Ok((jar, Json(mt_timer)))
        }
        (Err(e), Ok(Some(_))) => {
            tracing::warn!("failed to fetch milltime timer, but found in db: {:?}", e);
            match e {
                MilltimeFetchError::ResponseError(_) => {
                    tracing::warn!("response error, not deleting active timer");
                }
                _ => milltime_repo.delete_active_timer(&user.id).await.unwrap(),
            }

            Err(ErrorResponse {
                status: StatusCode::NOT_FOUND,
                error: MilltimeError::TimerError,
                message: "timer found in db but not on milltime".to_string(),
            })
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

    let user = auth_session.user.expect("user not found");
    let new_timer = repositories::NewMilltimeTimer {
        user_id: user.id,
        start_time: time::OffsetDateTime::now_utc(),
        project_id: body.project_id.clone(),
        project_name: body.project_name.clone(),
        activity_id: body.activity.clone(),
        activity_name: body.activity_name.clone(),
        note: body.user_note.clone().unwrap_or_default(),
    };

    if let Err(e) = app_state.milltime_repo.create_timer(&new_timer).await {
        tracing::error!("failed to create timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
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
    let update_timer = repositories::UpdateMilltimeTimer {
        user_id: user.id,
        user_note: body.user_note,
    };
    if let Err(e) = app_state.milltime_repo.update_timer(&update_timer).await {
        tracing::error!("failed to update timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[instrument(name = "get_timer_history", skip(auth_session, app_state))]
pub async fn get_timer_history(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<repositories::MilltimeTimer>>, (StatusCode, String)> {
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
