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

use crate::{app_state::AppState, domain::MilltimePassword, repositories::RepositoryError};

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
                .delete(calendar::delete_project_registration),
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
            "/timer/standalone",
            post(timer::start_standalone_timer)
                .delete(timer::stop_standalone_timer)
                .put(timer::save_standalone_timer),
        )
        .route("/update-timer", put(timer::edit_timer))
        .route(
            "/update-timer/standalone",
            put(timer::edit_standalone_timer),
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

impl From<milltime::MilltimeFetchError> for ErrorResponse {
    fn from(error: milltime::MilltimeFetchError) -> Self {
        match error {
            milltime::MilltimeFetchError::Unauthorized => ErrorResponse {
                status: StatusCode::UNAUTHORIZED,
                error: MilltimeError::MilltimeAuthenticationFailed,
                message: "Unauthorized".to_string(),
            },
            _ => ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                error: MilltimeError::FetchError,
                message: error.to_string(),
            },
        }
    }
}

impl From<RepositoryError> for ErrorResponse {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound(_) => ErrorResponse {
                status: StatusCode::NOT_FOUND,
                error: MilltimeError::DatabaseError,
                message: error.to_string(),
            },
            _ => ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                error: MilltimeError::DatabaseError,
                message: error.to_string(),
            },
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let body = Json(&self);
        (self.status, body).into_response()
    }
}

type CookieJarResult<T> = Result<(CookieJar, T), ErrorResponse>;

trait MilltimeCookieJarExt: std::marker::Sized {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), ErrorResponse>;
    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self;
}

impl MilltimeCookieJarExt for CookieJar {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), ErrorResponse> {
        let (credentials, jar) = match self.clone().try_into() {
            Ok(c) => {
                tracing::debug!("using existing milltime credentials");
                (c, self)
            }
            Err(_) => {
                let user = self.get("mt_user").ok_or(ErrorResponse {
                    status: StatusCode::UNAUTHORIZED,
                    error: MilltimeError::MilltimeAuthenticationFailed,
                    message: "missing mt_user cookie".to_string(),
                })?;
                let pass = self.get("mt_password").ok_or(ErrorResponse {
                    status: StatusCode::UNAUTHORIZED,
                    error: MilltimeError::MilltimeAuthenticationFailed,
                    message: "missing mt_password cookie".to_string(),
                })?;
                let decrypted_pass = MilltimePassword::from_encrypted(pass.value().to_string());
                let creds = milltime::Credentials::new(user.value(), decrypted_pass.as_ref())
                    .await
                    .map_err(|e| {
                        tracing::error!("failed to create milltime credentials: {:?}", e);
                        ErrorResponse {
                            status: StatusCode::UNAUTHORIZED,
                            error: MilltimeError::MilltimeAuthenticationFailed,
                            message: e.to_string(),
                        }
                    })?;
                let jar = self.with_milltime_credentials(&creds, domain);

                tracing::debug!("created new milltime credentials");
                (creds, jar)
            }
        };

        Ok((milltime::MilltimeClient::new(credentials), jar))
    }

    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self {
        let mut jar = self.clone();
        for cookie in credentials.auth_cookies(domain.to_string()) {
            jar = jar.add(cookie);
        }

        jar
    }
}
