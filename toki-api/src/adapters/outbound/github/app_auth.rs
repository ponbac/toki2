//! GitHub App authentication: a short RS256 JWT signed with the app's private
//! key, exchanged for an installation token that is cached until shortly
//! before it expires.
//!
//! The JWT is signed with aws-lc-rs, never with the `rsa` crate (see
//! RUSTSEC-2023-0071 in `.cargo/audit.toml`). Keys, JWTs and tokens never
//! appear in logs or errors.

use std::fmt;

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{RsaKeyPair, RSA_PKCS1_SHA256},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use time::OffsetDateTime;
use tokio::sync::Mutex;

use super::client::{error_message, github_request, rate_limit, unavailable, GithubError};

/// A cached token is renewed once it expires within this long.
const RENEW_BEFORE: time::Duration = time::Duration::minutes(5);
/// The JWT's `iat` is backdated by this many seconds, against clock drift.
const JWT_BACKDATE_SECONDS: i64 = 60;
/// The JWT's `exp`, in seconds from now: GitHub accepts at most ten minutes
/// between `iat` and `exp`.
const JWT_LIFETIME_SECONDS: i64 = 540;

/// A secret whose `Debug` never shows it.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Redacted(String);

impl Redacted {
    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

struct InstallationToken {
    token: Redacted,
    expires_at: OffsetDateTime,
}

/// A GitHub App installation's credentials and its cached token.
pub(super) struct GithubAppAuth {
    client_id: String,
    installation_id: u64,
    key: RsaKeyPair,
    cached: Mutex<Option<InstallationToken>>,
}

#[derive(Deserialize)]
struct MintedToken {
    token: String,
    #[serde(with = "time::serde::rfc3339")]
    expires_at: OffsetDateTime,
}

impl GithubAppAuth {
    /// Parses the private key once. Fails with a message naming the setting.
    pub(super) fn new(
        client_id: &str,
        installation_id: u64,
        private_key_pem: &str,
    ) -> Result<Self, String> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err("github.app_client_id must not be blank".to_string());
        }
        Ok(Self {
            client_id: client_id.to_string(),
            installation_id,
            key: parse_private_key(private_key_pem)?,
            cached: Mutex::new(None),
        })
    }

    /// A JWT for the app, issued by its client ID, valid around `now`.
    pub(super) fn jwt(&self, now: OffsetDateTime) -> Result<Redacted, GithubError> {
        let now = now.unix_timestamp();
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = serde_json::json!({
            "iat": now - JWT_BACKDATE_SECONDS,
            "exp": now + JWT_LIFETIME_SECONDS,
            "iss": self.client_id,
        });
        let claims = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing_input = format!("{header}.{claims}");
        let mut signature = vec![0; self.key.public_modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signing_input.as_bytes(),
                &mut signature,
            )
            .map_err(|_| GithubError::Response("could not sign the GitHub App JWT".to_string()))?;
        Ok(Redacted(format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature)
        )))
    }

    /// The installation token, minted when there is none or it expires within
    /// `RENEW_BEFORE`. Concurrent callers share one mint.
    pub(super) async fn token(
        &self,
        http: &Client,
        api_base: &str,
    ) -> Result<Redacted, GithubError> {
        let mut cached = self.cached.lock().await;
        let now = OffsetDateTime::now_utc();
        if let Some(token) = cached
            .as_ref()
            .filter(|token| token.expires_at - RENEW_BEFORE > now)
        {
            return Ok(token.token.clone());
        }

        let minted = self.mint(http, api_base, now).await?;
        let token = minted.token.clone();
        *cached = Some(minted);
        Ok(token)
    }

    /// Drops the cached token if it is still `token`, after GitHub rejected
    /// it.
    pub(super) async fn invalidate(&self, token: &Redacted) {
        let mut cached = self.cached.lock().await;
        if cached.as_ref().is_some_and(|cached| cached.token == *token) {
            *cached = None;
        }
    }

    /// `POST /app/installations/{id}/access_tokens` with an empty body, so the
    /// token carries every permission the installation has.
    async fn mint(
        &self,
        http: &Client,
        api_base: &str,
        now: OffsetDateTime,
    ) -> Result<InstallationToken, GithubError> {
        let jwt = self.jwt(now)?;
        let response = github_request(http.post(format!(
            "{api_base}/app/installations/{}/access_tokens",
            self.installation_id
        )))
        .bearer_auth(jwt.expose())
        .send()
        .await
        .map_err(unavailable)?;

        let status = response.status();
        if status.is_success() {
            // The body holds the token: never echo it.
            let minted: MintedToken = response.json().await.map_err(|_| {
                GithubError::Response("unreadable installation token response".to_string())
            })?;
            return Ok(InstallationToken {
                token: Redacted(minted.token),
                expires_at: minted.expires_at,
            });
        }

        let headers = response.headers().clone();
        let message = error_message(response).await;
        Err(match status {
            StatusCode::UNAUTHORIZED => GithubError::Unauthorized(
                "GitHub rejected the app's JWT: check github.app_client_id and the private key"
                    .to_string(),
            ),
            StatusCode::NOT_FOUND => GithubError::NotFound(format!(
                "installation {} not found: check github.installation_id",
                self.installation_id
            )),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => rate_limit(&headers)
                .unwrap_or(if status == StatusCode::FORBIDDEN {
                    GithubError::Forbidden(format!(
                        "GitHub refused an installation token (is the app suspended?): {message}"
                    ))
                } else {
                    GithubError::RateLimited { retry_after: None }
                }),
            StatusCode::UNPROCESSABLE_ENTITY => GithubError::Unprocessable(message),
            status if status.is_server_error() => {
                GithubError::Unavailable(format!("GitHub answered {status}"))
            }
            status => GithubError::Response(format!(
                "GitHub answered {status} to the installation token request: {message}"
            )),
        })
    }
}

/// The app's RSA private key from PEM: `RSA PRIVATE KEY` (PKCS#1, as GitHub
/// generates it) or `PRIVATE KEY` (PKCS#8). A value without line breaks but
/// with literal `\n`, as some secret stores keep it, is unescaped first.
/// Errors never echo the input.
pub(super) fn parse_private_key(raw: &str) -> Result<RsaKeyPair, String> {
    let text = if !raw.contains('\n') && raw.contains("\\n") {
        raw.replace("\\n", "\n")
    } else {
        raw.to_string()
    };
    let block = pem::parse(text.trim())
        .map_err(|_| "github.app_private_key is not a PEM-encoded key".to_string())?;
    let parsed = match block.tag() {
        "RSA PRIVATE KEY" => RsaKeyPair::from_der(block.contents()),
        "PRIVATE KEY" => RsaKeyPair::from_pkcs8(block.contents()),
        tag => {
            return Err(format!(
                "github.app_private_key holds a {tag:?} PEM block, not an RSA private key"
            ))
        }
    };
    parsed.map_err(|rejected| {
        format!(
            "github.app_private_key is not a usable RSA private key ({})",
            rejected.description_()
        )
    })
}
