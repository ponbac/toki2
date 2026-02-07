mod authenticate;
mod calendar;
mod projects;
mod timer;

use axum::{
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use axum_extra::extract::CookieJar;
use reqwest::StatusCode;
use serde::Serialize;

use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate::authenticate))
        .route("/projects", get(projects::list_projects))
        .route(
            "/projects/:project_id/activities",
            get(projects::list_activities),
        )
        .route("/time-info", get(calendar::get_time_info))
        .route(
            "/time-entries",
            get(calendar::get_time_entries)
                .put(calendar::edit_project_registration)
                .delete(calendar::delete_project_registration)
                .post(calendar::create_project_registration),
        )
        .route("/timer-history", get(timer::get_timer_history))
        .route(
            "/timer",
            get(timer::get_timer)
                .post(timer::start_timer)
                .delete(timer::stop_timer)
                .put(timer::save_timer),
        )
        .route(
            "/update-timer",
            put(timer::edit_timer),
        )
}

#[derive(Debug, thiserror::Error, Serialize, strum::Display)]
enum MilltimeError {
    MilltimeAuthenticationFailed,
    TimerError,
    DateParseError,
    FetchError,
    DatabaseError,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    #[serde(skip)]
    status: StatusCode,
    error: MilltimeError,
    message: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let body = Json(&self);
        (self.status, body).into_response()
    }
}

type CookieJarResult<T> = Result<(CookieJar, T), ErrorResponse>;

trait MilltimeCookieJarExt: std::marker::Sized {
    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self;
}

impl MilltimeCookieJarExt for CookieJar {
    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self {
        let mut jar = self.clone();
        for cookie in credentials.auth_cookies(domain.to_string()) {
            jar = jar.add(cookie);
        }

        jar
    }
}
