//! Composition root — concrete factory for creating TimeTrackingService instances.
//!
//! This is the ONLY place that imports concrete outbound adapters and provider types.

use std::ops::Add;
use std::sync::Arc;

use async_trait::async_trait;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use time::{Duration, OffsetDateTime};

use crate::{
    adapters::{
        inbound::http::{TimeTrackingServiceError, TimeTrackingServiceFactory},
        outbound::{
            milltime::{MilltimeAdapter, MilltimePassword},
            postgres::PostgresTimerHistoryAdapter,
        },
    },
    domain::{
        ports::inbound::TimeTrackingService,
        services::TimeTrackingServiceImpl,
    },
    repositories::TimerRepositoryImpl,
};

/// Concrete factory that creates Milltime-backed TimeTrackingService instances.
pub struct MilltimeServiceFactory {
    timer_repo: Arc<TimerRepositoryImpl>,
}

impl MilltimeServiceFactory {
    pub fn new(timer_repo: Arc<TimerRepositoryImpl>) -> Self {
        Self { timer_repo }
    }
}

#[async_trait]
impl TimeTrackingServiceFactory for MilltimeServiceFactory {
    async fn create_service(
        &self,
        jar: CookieJar,
        cookie_domain: &str,
    ) -> Result<(Box<dyn TimeTrackingService>, CookieJar), TimeTrackingServiceError> {
        let (credentials, jar) = extract_credentials(jar, cookie_domain).await?;

        let adapter = MilltimeAdapter::new(credentials);
        let history_adapter = PostgresTimerHistoryAdapter::new(self.timer_repo.clone());
        let service =
            TimeTrackingServiceImpl::new(Arc::new(adapter), Arc::new(history_adapter));

        Ok((Box::new(service), jar))
    }

    async fn authenticate(
        &self,
        username: &str,
        password: &str,
        cookie_domain: &str,
    ) -> Result<CookieJar, TimeTrackingServiceError> {
        let credentials = milltime::Credentials::new(username, password)
            .await
            .map_err(|_| {
                TimeTrackingServiceError::unauthorized("Invalid credentials")
            })?;

        let encrypted_password = MilltimePassword::new(password.to_string()).to_encrypted();

        let use_secure = cookie_domain != "localhost";

        let mut jar = CookieJar::new()
            .add(
                Cookie::build(("mt_user", username.to_string()))
                    .domain(cookie_domain.to_string())
                    .path("/")
                    .secure(use_secure)
                    .http_only(true)
                    .expires(OffsetDateTime::now_utc().add(Duration::days(180)))
                    .build(),
            )
            .add(
                Cookie::build(("mt_password", encrypted_password))
                    .domain(cookie_domain.to_string())
                    .path("/")
                    .secure(use_secure)
                    .http_only(true)
                    .expires(OffsetDateTime::now_utc().add(Duration::days(180)))
                    .build(),
            );

        for cookie in credentials.auth_cookies(cookie_domain.to_string()) {
            jar = jar.add(cookie);
        }

        Ok(jar)
    }
}

/// Extract provider credentials from cookies.
///
/// First tries to parse existing credential cookies, then falls back to
/// username/password authentication.
async fn extract_credentials(
    jar: CookieJar,
    cookie_domain: &str,
) -> Result<(milltime::Credentials, CookieJar), TimeTrackingServiceError> {
    // Try to use existing credentials from cookies first
    if let Ok(credentials) = jar.clone().try_into() {
        tracing::debug!("using existing provider credentials");
        return Ok((credentials, jar));
    }

    // Fall back to username/password authentication
    let user_cookie = jar.get("mt_user").ok_or_else(|| {
        TimeTrackingServiceError::unauthorized("missing mt_user cookie")
    })?;

    let pass_cookie = jar.get("mt_password").ok_or_else(|| {
        TimeTrackingServiceError::unauthorized("missing mt_password cookie")
    })?;

    let decrypted_pass = MilltimePassword::from_encrypted(pass_cookie.value().to_string());

    let credentials = milltime::Credentials::new(user_cookie.value(), decrypted_pass.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("failed to create provider credentials: {:?}", e);
            TimeTrackingServiceError::unauthorized(e.to_string())
        })?;

    // Update cookies with the new credentials
    let mut updated_jar = jar;
    for cookie in credentials.auth_cookies(cookie_domain.to_string()) {
        updated_jar = updated_jar.add(cookie);
    }

    tracing::debug!("created new provider credentials");
    Ok((credentials, updated_jar))
}
