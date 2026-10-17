//! Validation of Microsoft Entra ID access tokens issued for the MCP endpoint.
//!
//! The endpoint is an OAuth resource server: Entra issues v2 access tokens
//! whose audience is Toki's app registration, and each request's token is
//! checked for its signature, issuer, audience, tenant, expiry and scope.

use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::RwLock;

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
        {
            let cached = self.keys.read().await;
            if let Some(cached) = cached.as_ref() {
                let fresh = cached.fetched_at.elapsed() < KEY_MAX_AGE;
                match cached.keys.find(key_id) {
                    Some(jwk) if fresh => return decoding_key(jwk),
                    None if cached.fetched_at.elapsed() < KEY_REFRESH_INTERVAL => {
                        return Err(EntraTokenError::Invalid("unknown signing key".into()));
                    }
                    _ => {}
                }
            }
        }

        let keys = self.fetch_keys().await?;
        let key = keys
            .find(key_id)
            .map(decoding_key)
            .unwrap_or_else(|| Err(EntraTokenError::Invalid("unknown signing key".into())));
        *self.keys.write().await = Some(CachedKeys {
            keys,
            fetched_at: Instant::now(),
        });
        key
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

    // A throwaway 2048-bit RSA key, used only to sign test tokens.
    const PRIVATE_KEY: &str = include_str!("testdata/entra_test_key.pem");
    const MODULUS: &str = include_str!("testdata/entra_test_key.n");

    async fn validator() -> EntraTokenValidator {
        let keys: JwkSet = serde_json::from_value(json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "kid": KEY_ID,
                "n": MODULUS.trim(),
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
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap()
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
