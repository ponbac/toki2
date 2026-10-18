use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::State,
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use super::{no_store, oauth_error, ServerState};
use crate::domain::{
    models::{ClientRegistration, RegisteredClient},
    OAuthError,
};

pub(super) const REGISTRATIONS_PER_WINDOW: usize = 300;
const REGISTRATION_WINDOW: Duration = Duration::from_secs(60 * 60);
/// Real registrations take a few hundred bytes. Unused ones are kept for 30
/// days, so this bounds what anonymous callers can store: 300 an hour for 30
/// days at 4 KB is under 1 GB.
pub(super) const MAX_REGISTRATION_BYTES: usize = 4 * 1024;

/// Caps registrations across all callers within a rolling window, bounding
/// how fast anonymous registration can grow the database. Each stored
/// registration holds its slot for the whole window. One caller can use up
/// the allowance, which then blocks only new registrations.
pub(super) struct RegistrationLimiter {
    limit: usize,
    window: Duration,
    recent: Mutex<VecDeque<Instant>>,
}

impl RegistrationLimiter {
    pub(super) fn hourly() -> Self {
        Self::new(REGISTRATIONS_PER_WINDOW, REGISTRATION_WINDOW)
    }

    fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            recent: Mutex::new(VecDeque::with_capacity(limit)),
        }
    }

    /// Reserves a slot at `now`, or says how long until one frees up.
    fn acquire(&self, now: Instant) -> Result<Instant, Duration> {
        let mut recent = self
            .recent
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while recent
            .front()
            .is_some_and(|&at| now.saturating_duration_since(at) >= self.window)
        {
            recent.pop_front();
        }
        if recent.len() >= self.limit {
            let oldest = recent.front().copied().unwrap_or(now);
            return Err(self
                .window
                .saturating_sub(now.saturating_duration_since(oldest)));
        }
        recent.push_back(now);
        Ok(now)
    }

    /// Frees a slot whose registration was refused.
    fn release(&self, at: Instant) {
        let mut recent = self
            .recent
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = recent.iter().rposition(|&slot| slot == at) {
            recent.remove(index);
        }
    }
}

#[derive(Debug, Deserialize)]
struct RegistrationRequest {
    redirect_uris: Option<Vec<String>>,
    client_name: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
    token_endpoint_auth_method: Option<String>,
}

impl From<RegistrationRequest> for ClientRegistration {
    fn from(request: RegistrationRequest) -> Self {
        Self {
            redirect_uris: request.redirect_uris,
            client_name: request.client_name,
            grant_types: request.grant_types,
            response_types: request.response_types,
            token_endpoint_auth_method: request.token_endpoint_auth_method,
        }
    }
}

#[derive(Debug, Serialize)]
struct RegistrationResponse {
    client_id: String,
    client_id_issued_at: i64,
    client_name: String,
    redirect_uris: Vec<String>,
    grant_types: [&'static str; 2],
    response_types: [&'static str; 1],
    token_endpoint_auth_method: &'static str,
}

impl From<RegisteredClient> for RegistrationResponse {
    fn from(registered: RegisteredClient) -> Self {
        Self {
            client_id: registered.client.id,
            client_id_issued_at: registered.created_at.unix_timestamp(),
            client_name: registered.client.name,
            redirect_uris: registered
                .client
                .redirect_uris
                .iter()
                .map(|uri| uri.as_str().to_string())
                .collect(),
            grant_types: ["authorization_code", "refresh_token"],
            response_types: ["code"],
            token_endpoint_auth_method: "none",
        }
    }
}

/// `POST /oauth/register`: dynamic registration of a public client.
pub(super) async fn register(State(state): State<ServerState>, body: Body) -> Response {
    let body = axum::body::to_bytes(body, MAX_REGISTRATION_BYTES)
        .await
        .map_err(|_| format!("The registration exceeds {MAX_REGISTRATION_BYTES} bytes."));
    let request = match body.map(|body| {
        serde_json::from_slice::<RegistrationRequest>(&body)
            .map_err(|error| format!("The registration is not valid client metadata: {error}."))
    }) {
        Ok(Ok(request)) => request,
        Ok(Err(message)) | Err(message) => return invalid_client_metadata(&message),
    };

    let slot = match state.registrations.acquire(Instant::now()) {
        Ok(slot) => slot,
        Err(retry_after) => return too_many_registrations(retry_after),
    };
    match state.service.register_client(request.into()).await {
        Ok(registered) => no_store(
            (
                StatusCode::CREATED,
                Json(RegistrationResponse::from(registered)),
            )
                .into_response(),
        ),
        Err(error) => {
            state.registrations.release(slot);
            registration_error(error)
        }
    }
}

fn registration_error(error: OAuthError) -> Response {
    match error {
        OAuthError::InvalidRedirectUri(message) => {
            oauth_error(StatusCode::BAD_REQUEST, "invalid_redirect_uri", &message)
        }
        OAuthError::Storage(message) => {
            tracing::error!("OAuth client registration failed: {message}");
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "The client could not be registered.",
            )
        }
        error => invalid_client_metadata(&error.to_string()),
    }
}

fn invalid_client_metadata(description: &str) -> Response {
    oauth_error(
        StatusCode::BAD_REQUEST,
        "invalid_client_metadata",
        description,
    )
}

fn too_many_registrations(retry_after: Duration) -> Response {
    let mut response = oauth_error(
        StatusCode::TOO_MANY_REQUESTS,
        "too_many_requests",
        "Too many clients were registered recently. Try again later.",
    );
    let seconds = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from(seconds.max(1)));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limiter_admits_up_to_its_limit_per_rolling_window() {
        let limiter = RegistrationLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();

        assert!(limiter.acquire(start).is_ok());
        assert!(limiter.acquire(start + Duration::from_secs(10)).is_ok());
        assert_eq!(
            limiter.acquire(start + Duration::from_secs(20)),
            Err(Duration::from_secs(40))
        );
        // The first slot frees up a window after it was taken.
        assert!(limiter.acquire(start + Duration::from_secs(60)).is_ok());
        assert!(limiter.acquire(start + Duration::from_secs(61)).is_err());
    }

    #[test]
    fn a_refused_registration_frees_its_slot() {
        let limiter = RegistrationLimiter::new(1, Duration::from_secs(60));
        let start = Instant::now();

        let slot = limiter.acquire(start).unwrap();
        assert!(limiter.acquire(start).is_err());
        limiter.release(slot);
        assert!(limiter.acquire(start).is_ok());
    }

    #[test]
    fn retry_after_rounds_up_to_whole_seconds() {
        let response = too_many_registrations(Duration::from_millis(1500));
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[RETRY_AFTER], "2");

        let response = too_many_registrations(Duration::ZERO);
        assert_eq!(response.headers()[RETRY_AFTER], "1");
    }
}
