use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use serde::Deserialize;
use tracing::instrument;

use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate))
        .route("/projects", get(list_projects))
        .route("/projects/:project_id/activities", get(list_activities))
        .route("/timer", post(start_timer))
        .route("/timer", delete(stop_timer))
        .route("/timer", put(save_timer))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatePayload {
    username: String,
    password: String,
}

#[instrument(name = "authenticate")]
async fn authenticate(
    jar: CookieJar,
    Json(body): Json<AuthenticatePayload>,
) -> Result<(CookieJar, StatusCode), (StatusCode, String)> {
    let credentials = milltime::Credentials::new(&body.username, &body.password).await;
    match credentials {
        Ok(creds) => {
            let mut jar = jar
                .add(Cookie::new("mt_user", body.username.clone()))
                .add(Cookie::new("mt_password", body.password.clone()));
            // TODO: real domain
            // for cookie in creds.auth_cookies(".ponbac.xyz") {
            for cookie in creds.auth_cookies("localhost") {
                jar = jar.add(cookie);
            }
            Ok((jar, StatusCode::OK))
        }
        Err(_) => Err((StatusCode::BAD_REQUEST, "Invalid credentials".to_string())),
    }
}

#[instrument(name = "list_projects")]
async fn list_projects(
    jar: CookieJar,
) -> Result<Json<Vec<milltime::ProjectSearchItem>>, (StatusCode, String)> {
    let milltime_client = jar.into_milltime_client().await;

    let search_filter = milltime::ProjectSearchFilter::new("Overview".to_string());
    let projects = milltime_client
        .fetch_project_search(search_filter)
        .await
        .unwrap()
        .into_iter()
        .filter(|project| project.is_member)
        .collect();

    Ok(Json(projects))
}

#[instrument(name = "list_activities")]
async fn list_activities(
    Path(project_id): Path<String>,
    jar: CookieJar,
) -> Result<Json<Vec<milltime::Activity>>, (StatusCode, String)> {
    let milltime_client = jar.into_milltime_client().await;

    let activity_filter = milltime::ActivityFilter::new(
        project_id,
        "2024-04-15".to_string(),
        "2024-04-21".to_string(),
    );
    let activities = milltime_client
        .fetch_activities(activity_filter)
        .await
        .unwrap();

    Ok(Json(activities))
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
    let milltime_client = jar.into_milltime_client().await;

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

#[instrument(name = "stop_timer")]
async fn stop_timer(jar: CookieJar) -> Result<StatusCode, (StatusCode, String)> {
    let milltime_client = jar.into_milltime_client().await;

    milltime_client.stop_timer().await.unwrap();

    Ok(StatusCode::OK)
}

#[instrument(name = "save_timer")]
async fn save_timer(jar: CookieJar) -> Result<StatusCode, (StatusCode, String)> {
    let milltime_client = jar.into_milltime_client().await;

    milltime_client.save_timer().await.unwrap();

    Ok(StatusCode::OK)
}

trait CookieJarExt {
    async fn into_milltime_client(self) -> milltime::MilltimeClient;
}

impl CookieJarExt for CookieJar {
    async fn into_milltime_client(self) -> milltime::MilltimeClient {
        let credentials = match self.clone().try_into() {
            Ok(c) => c,
            Err(_) => {
                let user = self.get("mt_user").expect("User cookie not found");
                let pass = self.get("mt_password").expect("Password cookie not found");
                milltime::Credentials::new(user.value(), pass.value())
                    .await
                    .expect("Invalid credentials")
            }
        };

        milltime::MilltimeClient::new(credentials)
    }
}
