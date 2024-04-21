use axum::{debug_handler, http::StatusCode, routing::post, Json, Router};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use serde::Deserialize;
use tracing::instrument;

use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate))
        .route("/timer", post(start_timer))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatePayload {
    username: String,
    password: String,
}

#[instrument(name = "authenticate")]
#[debug_handler]
async fn authenticate(
    jar: CookieJar,
    Json(body): Json<AuthenticatePayload>,
) -> Result<(CookieJar, StatusCode), (StatusCode, String)> {
    let credentials = milltime::Credentials::new(&body.username, &body.password).await;
    match credentials {
        Ok(_creds) => {
            let jar = jar
                .add(Cookie::new("mt_user", body.username.clone()))
                .add(Cookie::new("mt_password", body.password.clone()));
            Ok((jar, StatusCode::OK))
        }
        Err(_) => Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartTimerPayload {
    activity: String,
    activity_name: String,
    project_id: String,
    project_name: String,
    user_id: String,
    user_note: Option<String>,
    reg_day: String,
    week_number: i64,
}

#[instrument(name = "start_timer")]
async fn start_timer(
    jar: CookieJar,
    Json(body): Json<StartTimerPayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = jar.get("mt_user").expect("User cookie not found");
    let pass = jar.get("mt_password").expect("Password cookie not found");

    let credentials = milltime::Credentials::new(user.value(), pass.value())
        .await
        .expect("Invalid credentials");
    let milltime_client = milltime::MilltimeClient::new(credentials);

    let start_timer_options = milltime::StartTimerOptions::new(
        body.activity.clone(),
        body.activity_name.clone(),
        body.project_id.clone(),
        body.project_name.clone(),
        body.user_id.clone(),
        body.user_note.clone(),
        body.reg_day.clone(),
        body.week_number,
    );

    milltime_client
        .start_timer(start_timer_options)
        .await
        .unwrap();

    Ok(StatusCode::OK)
}
