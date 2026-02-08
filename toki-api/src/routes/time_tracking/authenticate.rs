use axum::{
    extract::State,
    http::StatusCode,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use tracing::instrument;

use crate::{app_state::AppState, routes::ApiError};

use super::CookieJarResult;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatePayload {
    username: String,
    password: String,
}

#[instrument(name = "authenticate", skip(jar, app_state))]
pub async fn authenticate(
    State(app_state): State<AppState>,
    jar: CookieJar,
    axum::Json(body): axum::Json<AuthenticatePayload>,
) -> CookieJarResult<StatusCode> {
    let result = app_state
        .time_tracking_factory
        .authenticate(&body.username, &body.password, &app_state.cookie_domain)
        .await;

    match result {
        Ok(auth_jar) => {
            // Merge auth cookies into the existing jar
            let mut merged = jar;
            for cookie in auth_jar.iter() {
                merged = merged.add(cookie.clone());
            }
            Ok((merged, StatusCode::OK))
        }
        Err(e) => Err(ApiError::from(e)),
    }
}
