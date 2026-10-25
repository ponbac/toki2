//! An HTTP client for GitHub's REST API, authenticated as a GitHub App
//! installation. Shared through `Arc` by every GitHub adapter, so they share
//! one installation token.

use std::time::Duration;

use reqwest::{
    header::{HeaderMap, ACCEPT, RETRY_AFTER},
    Client, Response, StatusCode,
};
use serde::de::DeserializeOwned;
use time::OffsetDateTime;

use super::{app_auth::GithubAppAuth, GithubAppCredentials};
use crate::domain::ProviderAccountDirectoryError;

/// The REST API version every request asks for.
const API_VERSION: &str = "2022-11-28";
const API_VERSION_HEADER: &str = "X-GitHub-Api-Version";
const ACCEPT_JSON: &str = "application/vnd.github+json";
const TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest wait a rate limit is reported with.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// The most characters of GitHub's own error message kept in an error.
const MAX_MESSAGE_CHARS: usize = 300;

/// A failed GitHub request. Adapter-local, so each port maps it to its own
/// error. Messages carry at most GitHub's own `message`, never a credential.
#[derive(Debug)]
pub(super) enum GithubError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Unprocessable(String),
    RateLimited {
        retry_after: Option<Duration>,
    },
    Unavailable(String),
    Response(String),
    /// The installation token could not be minted, for the inner reason.
    Token(Box<GithubError>),
}

impl From<GithubError> for ProviderAccountDirectoryError {
    fn from(error: GithubError) -> Self {
        match error {
            GithubError::Unauthorized(message) => Self::Unauthorized(message),
            GithubError::Forbidden(message) => Self::Forbidden(message),
            GithubError::NotFound(message) => Self::NotFound(message),
            GithubError::Unprocessable(message) => {
                Self::Response(format!("GitHub could not process the request: {message}"))
            }
            GithubError::RateLimited { retry_after } => Self::RateLimited { retry_after },
            GithubError::Unavailable(message) => Self::Unavailable(message),
            GithubError::Response(message) => Self::Response(message),
            GithubError::Token(inner) => (*inner).into(),
        }
    }
}

/// A successful response's body.
pub(super) struct GithubResponse {
    body: String,
}

impl GithubResponse {
    /// The body as JSON; malformed JSON is a `Response` error.
    pub(super) fn json<T: DeserializeOwned>(&self) -> Result<T, GithubError> {
        serde_json::from_str(&self.body)
            .map_err(|error| GithubError::Response(format!("unreadable GitHub response: {error}")))
    }
}

/// GitHub's REST API as one app installation.
pub(super) struct GithubAppClient {
    http: Client,
    api_base: String,
    auth: GithubAppAuth,
}

impl GithubAppClient {
    /// Fails, with a message naming the setting, when the credentials cannot
    /// be used: an unusable private key or API base.
    pub(super) fn new(credentials: &GithubAppCredentials) -> Result<Self, String> {
        let api_base = credentials
            .api_base
            .trim()
            .trim_end_matches('/')
            .to_string();
        let parsed = url::Url::parse(&api_base)
            .map_err(|_| format!("github.api_base {api_base:?} is not a URL"))?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err(format!("github.api_base {api_base:?} is not an HTTP URL"));
        }
        let auth = GithubAppAuth::new(
            &credentials.client_id,
            credentials.installation_id,
            &credentials.private_key_pem,
        )?;
        let http = Client::builder()
            .user_agent(concat!("toki-api/", env!("CARGO_PKG_VERSION")))
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            // The token goes to `api_base` only, never to where it redirects.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("could not build the GitHub client: {error}"))?;

        Ok(Self {
            http,
            api_base,
            auth,
        })
    }

    /// `GET {api_base}{path}` with the installation token. A `401` drops the
    /// token, and the request is retried once with a new one.
    pub(super) async fn get(&self, path: &str) -> Result<GithubResponse, GithubError> {
        let token = self.token().await?;
        let mut response = self.send_get(path, token.expose()).await?;
        if response.status() == StatusCode::UNAUTHORIZED {
            self.auth.invalidate(&token).await;
            let token = self.token().await?;
            response = self.send_get(path, token.expose()).await?;
        }
        read(response).await
    }

    async fn token(&self) -> Result<super::app_auth::Redacted, GithubError> {
        self.auth
            .token(&self.http, &self.api_base)
            .await
            .map_err(|error| GithubError::Token(Box::new(error)))
    }

    async fn send_get(&self, path: &str, token: &str) -> Result<Response, GithubError> {
        github_request(self.http.get(format!("{}{path}", self.api_base)))
            .bearer_auth(token)
            .send()
            .await
            .map_err(unavailable)
    }
}

/// A request with the headers GitHub asks every client to send. The
/// `User-Agent`, which GitHub requires, is the client's.
pub(super) fn github_request(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request
        .header(ACCEPT, ACCEPT_JSON)
        .header(API_VERSION_HEADER, API_VERSION)
}

/// A request that never reached GitHub or never got an answer.
pub(super) fn unavailable(error: reqwest::Error) -> GithubError {
    GithubError::Unavailable(format!("GitHub could not be reached: {error}"))
}

/// The body of a successful response, or the status mapped to an error.
async fn read(response: Response) -> Result<GithubResponse, GithubError> {
    let status = response.status();
    if status.is_success() {
        return response
            .text()
            .await
            .map(|body| GithubResponse { body })
            .map_err(unavailable);
    }

    let headers = response.headers().clone();
    let message = error_message(response).await;
    Err(match status {
        StatusCode::UNAUTHORIZED => {
            GithubError::Unauthorized("GitHub rejected the installation token".to_string())
        }
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            rate_limit(&headers).unwrap_or(if status == StatusCode::FORBIDDEN {
                GithubError::Forbidden(message)
            } else {
                GithubError::RateLimited { retry_after: None }
            })
        }
        StatusCode::NOT_FOUND => GithubError::NotFound(message),
        StatusCode::UNPROCESSABLE_ENTITY => GithubError::Unprocessable(message),
        status if status.is_server_error() => {
            GithubError::Unavailable(format!("GitHub answered {status}"))
        }
        status => GithubError::Response(format!("GitHub answered {status}: {message}")),
    })
}

/// GitHub's own `message` from an error body, shortened, or the status's
/// reason when there is none.
pub(super) async fn error_message(response: Response) -> String {
    let status = response.status();
    let message = response
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|body| body.get("message")?.as_str().map(str::to_string))
        .map(|message| {
            message
                .trim()
                .chars()
                .take(MAX_MESSAGE_CHARS)
                .collect::<String>()
        })
        .filter(|message| !message.is_empty());
    message.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("no message")
            .to_string()
    })
}

/// A rate limit, when a `403` or `429` is one: a `Retry-After` in seconds
/// (a secondary limit), or no requests remaining until `x-ratelimit-reset`
/// (the primary limit). Waits are capped at an hour.
pub(super) fn rate_limit(headers: &HeaderMap) -> Option<GithubError> {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    if let Some(seconds) = header(RETRY_AFTER.as_str()).and_then(|value| value.trim().parse().ok())
    {
        return Some(GithubError::RateLimited {
            retry_after: Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER)),
        });
    }
    if header("x-ratelimit-remaining").map(str::trim) == Some("0") {
        let retry_after = header("x-ratelimit-reset")
            .and_then(|value| value.trim().parse::<i64>().ok())
            .map(|reset| {
                let wait = reset - OffsetDateTime::now_utc().unix_timestamp();
                Duration::from_secs(u64::try_from(wait).unwrap_or(0)).min(MAX_RETRY_AFTER)
            });
        return Some(GithubError::RateLimited { retry_after });
    }
    None
}
