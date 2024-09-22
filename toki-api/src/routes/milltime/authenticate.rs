use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use serde::Deserialize;
use std::ops::Add;
use time::{Duration, OffsetDateTime};
use tracing::instrument;

use crate::{app_state::AppState, domain::MilltimePassword};

use super::{CookieJarResult, ErrorResponse, MilltimeCookieJarExt, MilltimeError};

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
    Json(body): Json<AuthenticatePayload>,
) -> CookieJarResult<StatusCode> {
    let credentials = milltime::Credentials::new(&body.username, &body.password).await;
    match credentials {
        Ok(creds) => {
            let domain = app_state.cookie_domain;
            let encrypted_password = MilltimePassword::new(body.password.clone()).to_encrypted();
            let mut jar = jar
                .add(
                    Cookie::build(("mt_user", body.username))
                        .domain(domain.clone())
                        .path("/")
                        .secure(true)
                        .http_only(false)
                        .expires(OffsetDateTime::now_utc().add(Duration::days(30)))
                        .build(),
                )
                .add(
                    Cookie::build(("mt_password", encrypted_password))
                        .domain(domain.clone())
                        .path("/")
                        .secure(true)
                        .http_only(true)
                        .expires(OffsetDateTime::now_utc().add(Duration::days(30)))
                        .build(),
                );
            jar = jar.with_milltime_credentials(&creds, &domain);

            Ok((jar, StatusCode::OK))
        }
        Err(_) => Err(ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            error: MilltimeError::MilltimeAuthenticationFailed,
            message: "Invalid credentials".to_string(),
        }),
    }
}
