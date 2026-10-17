//! Validation of Microsoft Entra ID access tokens issued for the MCP endpoint.
//!
//! The endpoint is an OAuth resource server: Entra issues v2 access tokens
//! whose audience is Toki's app registration, and each request's token is
//! checked for its signature, issuer, audience, tenant, expiry and scope.

use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

/// Signing keys are re-read at most this often when a token names an unknown key.
const KEY_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// Signing keys are re-read after this long even when every key is known.
const KEY_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const CLOCK_SKEW_SECONDS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum EntraTokenError {
    #[error("the access token is invalid: {0}")]
    Invalid(String),
    #[error("the access token lacks the {0} scope")]
    MissingScope(String),
    #[error("Entra signing keys are unavailable: {0}")]
    Keys(String),
}

/// The caller an accepted token names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntraIdentity {
    pub object_id: String,
    /// The token's sign-in name, then its email when that differs. Toki users
    /// are known by the email their web sign-in reported.
    pub emails: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    tid: String,
    oid: String,
    #[serde(default)]
    scp: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

struct CachedKeys {
    keys: JwkSet,
    fetched_at: Instant,
}

pub struct EntraTokenValidator {
    tenant_id: String,
    audience: String,
    issuer: String,
    scope: String,
    keys_url: String,
    http: reqwest::Client,
    keys: RwLock<Option<CachedKeys>>,
    /// Held while re-reading the keys, so concurrent requests share one fetch.
    /// Holds when the last fetch started, successful or not.
    refresh: Mutex<Option<Instant>>,
}

impl EntraTokenValidator {
    /// Accepts tokens from `tenant_id` for the app `audience` carrying `scope`.
    pub fn new(tenant_id: &str, audience: &str, scope: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            audience: audience.to_string(),
            issuer: issuer(tenant_id),
            scope: scope.to_string(),
            keys_url: format!("https://login.microsoftonline.com/{tenant_id}/discovery/v2.0/keys"),
            http: reqwest::Client::new(),
            keys: RwLock::new(None),
            refresh: Mutex::new(None),
        }
    }

    pub async fn validate(&self, token: &str) -> Result<EntraIdentity, EntraTokenError> {
        let header =
            decode_header(token).map_err(|error| EntraTokenError::Invalid(error.to_string()))?;
        if header.alg != Algorithm::RS256 {
            return Err(EntraTokenError::Invalid(format!(
                "unexpected algorithm {:?}",
                header.alg
            )));
        }
        let key_id = header
            .kid
            .ok_or_else(|| EntraTokenError::Invalid("the token names no signing key".into()))?;
        let key = self.decoding_key(&key_id).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.leeway = CLOCK_SKEW_SECONDS;
        let claims = decode::<Claims>(token, &key, &validation)
            .map_err(|error| EntraTokenError::Invalid(error.to_string()))?
            .claims;

        if claims.tid != self.tenant_id {
            return Err(EntraTokenError::Invalid(
                "the token is from another tenant".into(),
            ));
        }
        let has_scope = claims
            .scp
            .as_deref()
            .is_some_and(|scopes| scopes.split_whitespace().any(|scope| scope == self.scope));
        if !has_scope {
            return Err(EntraTokenError::MissingScope(self.scope.clone()));
        }
        let mut emails: Vec<String> = Vec::new();
        for email in [claims.preferred_username, claims.email]
            .into_iter()
            .flatten()
        {
            if !email.is_empty()
                && !emails
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(&email))
            {
                emails.push(email);
            }
        }
        if emails.is_empty() {
            return Err(EntraTokenError::Invalid("the token names no user".into()));
        }

        Ok(EntraIdentity {
            object_id: claims.oid,
            emails,
        })
    }

    async fn decoding_key(&self, key_id: &str) -> Result<DecodingKey, EntraTokenError> {
        if let Some(key) = self.cached_key(key_id, false).await {
            return key;
        }

        let mut last_fetch = self.refresh.lock().await;
        // Another request may have refreshed the keys while this one waited.
        if let Some(key) = self.cached_key(key_id, false).await {
            return key;
        }
        // A fetch just ran and failed: serve the stale keys, or fail, until the
        // next refresh is due, rather than asking Entra on every request.
        if last_fetch.is_some_and(|started| started.elapsed() < KEY_REFRESH_INTERVAL) {
            return self.cached_key(key_id, true).await.unwrap_or_else(|| {
                Err(EntraTokenError::Keys(
                    "the last signing key fetch failed".into(),
                ))
            });
        }

        *last_fetch = Some(Instant::now());
        match self.fetch_keys().await {
            Ok(keys) => {
                let key = keys
                    .find(key_id)
                    .map(decoding_key)
                    .unwrap_or_else(|| Err(unknown_key()));
                *self.keys.write().await = Some(CachedKeys {
                    keys,
                    fetched_at: Instant::now(),
                });
                key
            }
            Err(error) => self.cached_key(key_id, true).await.unwrap_or(Err(error)),
        }
    }

    /// The cached answer for `key_id`, when the cache can give one: its key
    /// while fresh, or that it is unknown right after a fetch. With `stale`,
    /// any cached answer.
    async fn cached_key(
        &self,
        key_id: &str,
        stale: bool,
    ) -> Option<Result<DecodingKey, EntraTokenError>> {
        let cached = self.keys.read().await;
        let cached = cached.as_ref()?;
        let age = cached.fetched_at.elapsed();
        match cached.keys.find(key_id) {
            Some(jwk) if stale || age < KEY_MAX_AGE => Some(decoding_key(jwk)),
            None if stale || age < KEY_REFRESH_INTERVAL => Some(Err(unknown_key())),
            _ => None,
        }
    }

    async fn fetch_keys(&self) -> Result<JwkSet, EntraTokenError> {
        self.http
            .get(&self.keys_url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| EntraTokenError::Keys(error.to_string()))?
            .json::<JwkSet>()
            .await
            .map_err(|error| EntraTokenError::Keys(error.to_string()))
    }

    #[cfg(test)]
    fn with_keys_url(mut self, keys_url: String) -> Self {
        self.keys_url = keys_url;
        self
    }

    #[cfg(test)]
    async fn with_keys(self, keys: JwkSet) -> Self {
        *self.keys.write().await = Some(CachedKeys {
            keys,
            fetched_at: Instant::now(),
        });
        self
    }
}

/// The issuer of the tenant's v2 access tokens, and the authorization server
/// MCP clients discover.
pub fn issuer(tenant_id: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant_id}/v2.0")
}

fn unknown_key() -> EntraTokenError {
    EntraTokenError::Invalid("unknown signing key".into())
}

fn decoding_key(jwk: &jsonwebtoken::jwk::Jwk) -> Result<DecodingKey, EntraTokenError> {
    DecodingKey::from_jwk(jwk).map_err(|error| EntraTokenError::Keys(error.to_string()))
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::{json, Value};

    use super::*;

    const TENANT: &str = "d89ef75c-38db-4904-9d78-b872502ca145";
    const AUDIENCE: &str = "6494e716-bbf2-4164-a1f0-cd682313085b";
    const KEY_ID: &str = "test-key";

    /// A signing key generated for the test run, as PKCS#1 DER, and its modulus.
    static SIGNING_KEY: std::sync::LazyLock<(Vec<u8>, Vec<u8>)> = std::sync::LazyLock::new(|| {
        use aws_lc_rs::encoding::AsDer;

        let key = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
        let pkcs8 = key.as_der().unwrap();
        // PrivateKeyInfo ::= SEQUENCE { version, algorithm, privateKey OCTET STRING }
        let (_, info, _) = der_element(pkcs8.as_ref());
        let (_, _, rest) = der_element(info);
        let (_, _, rest) = der_element(rest);
        let (_, rsa_private_key, _) = der_element(rest);
        // RSAPrivateKey ::= SEQUENCE { version, modulus, ... }
        let (_, fields, _) = der_element(rsa_private_key);
        let (_, _, rest) = der_element(fields);
        let (_, modulus, _) = der_element(rest);
        let modulus = modulus.strip_prefix(&[0]).unwrap_or(modulus);
        (rsa_private_key.to_vec(), modulus.to_vec())
    });

    /// Splits one DER element into its tag, contents and the bytes after it.
    fn der_element(input: &[u8]) -> (u8, &[u8], &[u8]) {
        let (tag, first) = (input[0], input[1]);
        let (length, header) = if first < 0x80 {
            (first as usize, 2)
        } else {
            let octets = (first & 0x7f) as usize;
            let length = input[2..2 + octets]
                .iter()
                .fold(0, |length, &byte| (length << 8) | byte as usize);
            (length, 2 + octets)
        };
        (
            tag,
            &input[header..header + length],
            &input[header + length..],
        )
    }

    async fn validator() -> EntraTokenValidator {
        let modulus = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            &SIGNING_KEY.1,
        );
        let keys: JwkSet = serde_json::from_value(json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "kid": KEY_ID,
                "n": modulus,
                "e": "AQAB",
            }]
        }))
        .unwrap();
        EntraTokenValidator::new(TENANT, AUDIENCE, "mcp")
            .with_keys(keys)
            .await
    }

    fn claims() -> Value {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        json!({
            "iss": issuer(TENANT),
            "aud": AUDIENCE,
            "tid": TENANT,
            "oid": "3e08c3ff-e791-4639-9480-4dbc0568e5c8",
            "scp": "mcp",
            "preferred_username": "ada@example.com",
            "iat": now,
            "nbf": now,
            "exp": now + 3600,
        })
    }

    fn sign(claims: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(KEY_ID.to_string());
        encode(&header, claims, &EncodingKey::from_rsa_der(&SIGNING_KEY.0)).unwrap()
    }

    fn with(mut claims: Value, field: &str, value: Value) -> Value {
        claims[field] = value;
        claims
    }

    #[tokio::test]
    async fn accepts_a_token_for_this_app_tenant_and_scope() {
        let identity = validator().await.validate(&sign(&claims())).await.unwrap();

        assert_eq!(identity.emails, ["ada@example.com"]);
        assert_eq!(identity.object_id, "3e08c3ff-e791-4639-9480-4dbc0568e5c8");
    }

    #[tokio::test]
    async fn accepts_the_scope_among_others() {
        let token = sign(&with(claims(), "scp", json!("openid mcp profile")));

        assert!(validator().await.validate(&token).await.is_ok());
    }

    #[tokio::test]
    async fn rejects_tokens_for_other_audiences_issuers_tenants_and_scopes() {
        let validator = validator().await;
        let other = "00000000-0000-0000-0000-000000000000";

        for claims in [
            with(
                claims(),
                "aud",
                json!("00000003-0000-0000-c000-000000000000"),
            ),
            with(claims(), "iss", json!(issuer(other))),
            with(claims(), "tid", json!(other)),
            with(claims(), "scp", json!("User.Read")),
            with(claims(), "exp", json!(1)),
        ] {
            assert!(
                validator.validate(&sign(&claims)).await.is_err(),
                "{claims}"
            );
        }
    }

    #[tokio::test]
    async fn concurrent_requests_share_one_key_fetch() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let fetches = Arc::new(AtomicUsize::new(0));
        let counter = fetches.clone();
        let keys = axum::Router::new().route(
            "/keys",
            axum::routing::get(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    axum::Json(json!({ "keys": [] }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let keys_url = format!("http://{}/keys", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, keys).await.unwrap() });

        let validator =
            Arc::new(EntraTokenValidator::new(TENANT, AUDIENCE, "mcp").with_keys_url(keys_url));
        let token = sign(&claims());
        let requests = (0..20).map(|_| {
            let validator = validator.clone();
            let token = token.clone();
            tokio::spawn(async move { validator.validate(&token).await })
        });
        for request in futures::future::join_all(requests).await {
            assert!(request.unwrap().is_err());
        }

        assert_eq!(fetches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_a_tampered_token() {
        let token = sign(&claims());
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let (header, _) = signed.split_once('.').unwrap();
        let forged_claims = with(claims(), "preferred_username", json!("eve@example.com"));
        let forged_payload = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            forged_claims.to_string(),
        );

        let forged = format!("{header}.{forged_payload}.{signature}");

        assert!(validator().await.validate(&forged).await.is_err());
    }
}
