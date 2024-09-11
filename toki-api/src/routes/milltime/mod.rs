mod authenticate;
mod calendar;
mod projects;
mod timer;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use axum_extra::extract::CookieJar;
use reqwest::StatusCode;

use crate::{app_state::AppState, domain::MilltimePassword};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate::authenticate))
        .route("/projects", get(projects::list_projects))
        .route(
            "/projects/:project_id/activities",
            get(projects::list_activities),
        )
        .route("/time-info", get(calendar::get_time_info))
        .route("/time-entries", get(calendar::get_time_entries))
        .route("/timer-history", get(timer::get_timer_history))
        .route("/timer", get(timer::get_timer))
        .route("/timer", post(timer::start_timer))
        .route("/timer", delete(timer::stop_timer))
        .route("/timer", put(timer::save_timer))
        .route("/update-timer", put(timer::edit_timer))
}

type CookieJarResult<T> = Result<(CookieJar, T), (StatusCode, String)>;

trait MilltimeCookieJarExt: std::marker::Sized {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), (StatusCode, String)>;
    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self;
}

impl MilltimeCookieJarExt for CookieJar {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), (StatusCode, String)> {
        let (credentials, jar) = match self.clone().try_into() {
            Ok(c) => {
                tracing::debug!("using existing milltime credentials");
                (c, self)
            }
            Err(_) => {
                let user = self.get("mt_user").ok_or((
                    StatusCode::UNAUTHORIZED,
                    "missing mt_user cookie".to_string(),
                ))?;
                let pass = self.get("mt_password").ok_or((
                    StatusCode::UNAUTHORIZED,
                    "missing mt_password cookie".to_string(),
                ))?;
                let decrypted_pass = MilltimePassword::from_encrypted(pass.value().to_string());
                let creds = milltime::Credentials::new(user.value(), decrypted_pass.as_ref())
                    .await
                    .map_err(|e| {
                        tracing::error!("failed to create milltime credentials: {:?}", e);
                        (StatusCode::UNAUTHORIZED, e.to_string())
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
