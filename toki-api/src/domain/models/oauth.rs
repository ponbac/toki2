//! Models for Toki's OAuth 2.1 authorization server, through which MCP clients
//! connect to `/mcp` on a user's behalf.
//!
//! Clients are public: they identify themselves with a Client ID Metadata
//! Document (an HTTPS URL serving their metadata) or a dynamic registration,
//! and prove possession with PKCE rather than a secret. Codes and tokens are
//! opaque random values of which only hashes are stored.

use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use url::{Host, Url};

use super::UserId;
use crate::domain::OAuthError;

/// The one scope Toki grants: acting as the user in the MCP tools.
pub const OAUTH_SCOPE: &str = "mcp";
pub const AUTHORIZATION_REQUEST_LIFETIME: Duration = Duration::minutes(10);
pub const AUTHORIZATION_CODE_LIFETIME: Duration = Duration::minutes(5);
pub const ACCESS_TOKEN_LIFETIME: Duration = Duration::hours(1);
/// A refresh token unused for this long expires.
pub const REFRESH_TOKEN_IDLE_LIFETIME: Duration = Duration::days(30);
/// A connection lasts at most this long; the user then connects again, which
/// signs them in with Entra again.
pub const GRANT_LIFETIME: Duration = Duration::days(365);
/// A rotated refresh token presented again within this window is refused
/// without revoking the connection: one client refreshing twice at once.
/// Later, its reuse means it leaked, and the connection is revoked.
pub const REFRESH_REUSE_GRACE: Duration = Duration::seconds(30);

const MAX_URI_LEN: usize = 2048;
const MAX_CLIENT_NAME_LEN: usize = 100;
pub const MAX_REDIRECT_URIS: usize = 10;
const SECRET_BYTES: usize = 32;

/// The private-use schemes of native MCP apps whose callbacks Toki accepts.
/// An allowlist rather than any scheme: operating systems route many schemes
/// to handlers of their own (`ms-settings:`, for one).
const NATIVE_APP_SCHEMES: &[&str] = &["cursor"];

/// Where a redirect URI sends the authorization response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectTarget {
    /// A web server, named by its host.
    Web,
    /// An app on the user's computer listening on a loopback port.
    Loopback,
    /// The native app on the user's computer that claims the URI's scheme,
    /// such as `cursor://` (RFC 8252, section 7.1).
    PrivateUseScheme,
}

/// A redirect URI a client registered: `https`, `http` on a loopback host, or
/// a known native app's private-use scheme.
///
/// The MCP specification allows only HTTPS and loopback redirects; Cursor's
/// `cursor://` callback is accepted too because the Cursor app registers it
/// alongside its other redirects and fails if any is refused. Any local app
/// can claim a scheme, as any local process can listen on a loopback port, so
/// the consent page warns about both alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri {
    raw: String,
    target: RedirectTarget,
}

impl RedirectUri {
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() > MAX_URI_LEN {
            return None;
        }
        let url = Url::parse(raw).ok()?;
        if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let target = match url.scheme() {
            "https" if url.host().is_some() => RedirectTarget::Web,
            "http" if is_loopback(&url) => RedirectTarget::Loopback,
            scheme if NATIVE_APP_SCHEMES.contains(&scheme) && url.host().is_some() => {
                RedirectTarget::PrivateUseScheme
            }
            _ => return None,
        };
        Some(Self {
            raw: raw.to_string(),
            target,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn target(&self) -> RedirectTarget {
        self.target
    }

    /// Whether `presented` may receive the response for this registered URI:
    /// the same string, or, for a loopback URI, the same URI on any port, as
    /// native apps listen on an ephemeral port (RFC 8252, section 7.3).
    pub fn matches(&self, presented: &str) -> bool {
        if self.raw == presented {
            return true;
        }
        if self.target != RedirectTarget::Loopback {
            return false;
        }
        let Ok(presented_url) = Url::parse(presented) else {
            return false;
        };
        let Ok(mut registered) = Url::parse(&self.raw) else {
            return false;
        };
        // An omitted port is the default one; the strings must still match.
        registered.set_port(presented_url.port()).is_ok() && registered.as_str() == presented
    }

    /// What the consent page names as the destination: the host for web and
    /// loopback redirects, the scheme for private-use ones.
    pub fn display_destination(&self) -> String {
        let Ok(url) = Url::parse(&self.raw) else {
            return self.raw.clone();
        };
        match self.target {
            RedirectTarget::Web => url.host_str().unwrap_or_default().to_string(),
            RedirectTarget::Loopback => match url.port() {
                Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
                None => url.host_str().unwrap_or_default().to_string(),
            },
            RedirectTarget::PrivateUseScheme => format!("{}://", url.scheme()),
        }
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain == "localhost",
        Some(Host::Ipv4(address)) => address == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(address)) => address == Ipv6Addr::LOCALHOST,
        None => false,
    }
}

/// An S256 PKCE code challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceChallenge(String);

impl PkceChallenge {
    pub fn parse_s256(raw: &str) -> Option<Self> {
        let digest = URL_SAFE_NO_PAD.decode(raw).ok()?;
        (raw.len() == 43 && digest.len() == 32).then(|| Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `verifier` is the one this challenge was derived from.
    pub fn verify(&self, verifier: &str) -> bool {
        if !is_valid_verifier(verifier) {
            return false;
        }
        let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        constant_time_eq(computed.as_bytes(), self.0.as_bytes())
    }
}

/// RFC 7636, section 4.1: 43 to 128 unreserved characters.
pub fn is_valid_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (l, r)| difference | (l ^ r))
            == 0
}

/// What an [`OAuthSecret`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthSecretKind {
    /// The handle of a consent page awaiting the user's decision.
    AuthorizationRequest,
    AuthorizationCode,
    AccessToken,
    RefreshToken,
}

impl OAuthSecretKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::AuthorizationRequest => "tmcp_ar_",
            Self::AuthorizationCode => "tmcp_ac_",
            Self::AccessToken => "tmcp_at_",
            Self::RefreshToken => "tmcp_rt_",
        }
    }
}

/// An opaque handle, code or token this server issues. Only its hash is
/// stored. Debug redacts the value.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthSecret(String);

impl OAuthSecret {
    pub fn generate(kind: OAuthSecretKind) -> Self {
        let mut bytes = [0u8; SECRET_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        let mut secret = String::with_capacity(kind.prefix().len() + SECRET_BYTES * 2);
        secret.push_str(kind.prefix());
        for byte in bytes {
            secret.push_str(&format!("{byte:02x}"));
        }
        Self(secret)
    }

    /// Parses a secret of `kind`, or `None` for anything else, including a
    /// secret of another kind.
    pub fn parse(kind: OAuthSecretKind, raw: &str) -> Option<Self> {
        let secret = raw.strip_prefix(kind.prefix())?;
        (secret.len() == SECRET_BYTES * 2
            && secret
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        .then(|| Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn hash(&self) -> OAuthSecretHash {
        OAuthSecretHash(Sha256::digest(self.0.as_bytes()).into())
    }
}

impl fmt::Debug for OAuthSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OAuthSecret([redacted])")
    }
}

/// The stored lookup key of an [`OAuthSecret`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OAuthSecretHash([u8; 32]);

impl OAuthSecretHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A client_id that is an HTTPS URL serving the client's metadata document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdUrl(Url);

impl ClientIdUrl {
    /// Accepts an HTTPS URL with a path, without credentials or a fragment, in
    /// canonical form, so the document's `client_id` can be compared to it as
    /// a string. Dot segments, which URL parsing removes, therefore fail.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() > MAX_URI_LEN {
            return None;
        }
        let url = Url::parse(raw).ok()?;
        let valid = url.scheme() == "https"
            && url.host_str().is_some()
            && url.path() != "/"
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.as_str() == raw;
        valid.then_some(Self(url))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn url(&self) -> &Url {
        &self.0
    }

    pub fn host(&self) -> &str {
        self.0.host_str().unwrap_or_default()
    }
}

/// The fields of a Client ID Metadata Document that Toki reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ClientMetadataDocument {
    pub client_id: String,
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
    /// Not a registered metadata field: ChatGPT lists the methods it can use,
    /// including `none`, beside its preferred `private_key_jwt`.
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

/// How a client identified itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthClientKind {
    /// By a metadata document at its HTTPS client_id, whose host vouches for it.
    MetadataDocument,
    /// By dynamic registration; its name is its own claim.
    Registered,
}

/// A client allowed to request authorization. Every client is public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClient {
    pub id: String,
    pub name: String,
    pub kind: OAuthClientKind,
    pub redirect_uris: Vec<RedirectUri>,
}

impl OAuthClient {
    /// Validates a fetched metadata document for `client_id`.
    pub fn from_metadata_document(
        client_id: &ClientIdUrl,
        document: &ClientMetadataDocument,
    ) -> Result<Self, OAuthError> {
        let invalid = |reason: &str| {
            OAuthError::InvalidClient(format!("The client's metadata document {reason}."))
        };
        if document.client_id != client_id.as_str() {
            return Err(invalid("names another client_id"));
        }
        // The methods the client can use: those it lists, else its one
        // declared method, which defaults to none.
        let accepts_none = match &document.token_endpoint_auth_methods_supported {
            Some(methods) => methods.iter().any(|method| method == "none"),
            None => matches!(
                document.token_endpoint_auth_method.as_deref(),
                None | Some("none")
            ),
        };
        if !accepts_none {
            return Err(invalid(
                "requires client authentication, which Toki does not offer",
            ));
        }
        let redirect_uris = parse_redirect_uris(&document.redirect_uris)
            .ok_or_else(|| invalid("has missing or invalid redirect_uris"))?;
        let name = match document.client_name.as_deref() {
            Some(name) => {
                parse_client_name(name).ok_or_else(|| invalid("has an invalid client_name"))?
            }
            None => client_id.host().to_string(),
        };

        Ok(Self {
            id: client_id.as_str().to_string(),
            name,
            kind: OAuthClientKind::MetadataDocument,
            redirect_uris,
        })
    }

    /// The registered redirect URI `presented` matches, if any.
    pub fn redirect_uri_for(&self, presented: &str) -> Option<&RedirectUri> {
        self.redirect_uris
            .iter()
            .find(|registered| registered.matches(presented))
    }

    /// The host that vouches for this client: its metadata document's host.
    pub fn verified_host(&self) -> Option<String> {
        match self.kind {
            OAuthClientKind::MetadataDocument => Url::parse(&self.id)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string)),
            OAuthClientKind::Registered => None,
        }
    }
}

/// 1 to [`MAX_REDIRECT_URIS`] valid redirect URIs, or `None`.
pub fn parse_redirect_uris(raw: &[String]) -> Option<Vec<RedirectUri>> {
    if raw.is_empty() || raw.len() > MAX_REDIRECT_URIS {
        return None;
    }
    raw.iter().map(|uri| RedirectUri::parse(uri)).collect()
}

/// A trimmed client name of 1 to 100 characters without control characters.
pub fn parse_client_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    (!name.is_empty()
        && name.chars().count() <= MAX_CLIENT_NAME_LEN
        && !name.chars().any(char::is_control))
    .then(|| name.to_string())
}

/// A dynamic client registration request (RFC 7591), as received.
#[derive(Debug, Clone, Default)]
pub struct ClientRegistration {
    pub redirect_uris: Option<Vec<String>>,
    pub client_name: Option<String>,
    pub grant_types: Option<Vec<String>>,
    pub response_types: Option<Vec<String>>,
    pub token_endpoint_auth_method: Option<String>,
}

/// A validated registration, ready to store under a new client_id.
#[derive(Debug, Clone)]
pub struct NewRegisteredClient {
    pub id: String,
    pub name: String,
    pub redirect_uris: Vec<RedirectUri>,
}

/// A stored dynamic registration.
#[derive(Debug, Clone)]
pub struct RegisteredClient {
    pub client: OAuthClient,
    pub created_at: OffsetDateTime,
}

/// The parameters of an authorization request, as received. `scope` is not
/// among them: Toki grants only `mcp`, whatever a client asks for.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationParams {
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub response_type: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub resource: Option<String>,
    pub state: Option<String>,
}

/// An authorization request that passed validation and awaits consent.
#[derive(Debug, Clone)]
pub struct ValidatedAuthorization {
    pub client: OAuthClient,
    /// The registered redirect URI the request matched.
    pub registered_redirect_uri: RedirectUri,
    /// The redirect URI as the client presented it, where the response goes.
    pub redirect_uri: String,
    pub code_challenge: PkceChallenge,
    pub resource: String,
    pub state: Option<String>,
}

/// Where an authorization response goes, and what it carries.
#[derive(Debug, Clone)]
pub struct AuthorizationResponse {
    pub redirect_uri: String,
    pub state: Option<String>,
    pub outcome: AuthorizationOutcome,
}

#[derive(Debug, Clone)]
pub enum AuthorizationOutcome {
    Code(OAuthSecret),
    Error {
        code: AuthorizationErrorCode,
        description: String,
    },
}

/// RFC 6749 section 4.1.2.1 and RFC 8707 error codes for authorization
/// responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationErrorCode {
    InvalidRequest,
    UnsupportedResponseType,
    InvalidTarget,
    AccessDenied,
}

impl AuthorizationErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedResponseType => "unsupported_response_type",
            Self::InvalidTarget => "invalid_target",
            Self::AccessDenied => "access_denied",
        }
    }
}

/// A shown consent page's request, to store until the user decides.
#[derive(Debug, Clone)]
pub struct NewAuthorizationRequest {
    pub id: OAuthSecretHash,
    pub user_id: UserId,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub resource: String,
    pub code_challenge: PkceChallenge,
    pub state: Option<String>,
    pub expires_at: OffsetDateTime,
}

/// A consumed consent request: where to send the user's decision.
#[derive(Debug, Clone)]
pub struct DecidedAuthorizationRequest {
    pub redirect_uri: String,
    pub state: Option<String>,
}

/// An authorization code to store when the user allows a request.
#[derive(Debug, Clone)]
pub struct NewAuthorizationCode {
    pub hash: OAuthSecretHash,
    pub expires_at: OffsetDateTime,
}

/// An authorization code as stored, locked for redemption.
#[derive(Debug, Clone)]
pub struct StoredAuthorizationCode {
    pub user_id: UserId,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub resource: String,
    pub code_challenge: PkceChallenge,
    pub expires_at: OffsetDateTime,
    pub consumed: bool,
}

/// What to do with a code being redeemed, decided on its locked row.
#[derive(Debug, Clone)]
pub enum CodeDecision {
    /// Create the grant and its tokens.
    Redeem,
    /// Refuse, keeping the code consumed.
    Reject(String),
    /// A replayed code: revoke the grant it produced, if any, and refuse.
    RevokeGrant(String),
}

/// A token about to be stored.
#[derive(Debug, Clone)]
pub struct NewToken {
    pub hash: OAuthSecretHash,
    pub expires_at: OffsetDateTime,
}

/// The tokens of a new or refreshed grant. A refresh token's expiry is capped
/// at its grant's.
#[derive(Debug, Clone)]
pub struct NewTokenPair {
    pub access: NewToken,
    pub refresh: NewToken,
}

#[derive(Debug, Clone)]
pub enum CodeRedemption {
    Unknown,
    Redeemed,
    Rejected(String),
}

/// A refresh token as stored with its grant, locked for rotation.
#[derive(Debug, Clone)]
pub struct StoredRefreshToken {
    pub client_id: String,
    pub resource: String,
    pub expires_at: OffsetDateTime,
    pub rotated_at: Option<OffsetDateTime>,
    pub grant_expires_at: OffsetDateTime,
    pub grant_revoked: bool,
}

/// What to do with a refresh token being exchanged, decided on its locked row.
#[derive(Debug, Clone)]
pub enum RefreshDecision {
    /// Mark it rotated and store the new pair.
    Rotate,
    /// Refuse without changing anything.
    Reject(String),
    /// Reuse of a rotated token: revoke the grant and refuse.
    RevokeGrant(String),
}

#[derive(Debug, Clone)]
pub enum RefreshRotation {
    Unknown,
    Rotated,
    Rejected(String),
}

/// Tokens issued to a client.
#[derive(Debug)]
pub struct IssuedTokens {
    pub access_token: OAuthSecret,
    pub refresh_token: OAuthSecret,
    pub expires_in: Duration,
}

/// A code exchange at the token endpoint, as received.
#[derive(Debug, Clone, Default)]
pub struct CodeExchange {
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub code_verifier: String,
    pub resource: Option<String>,
}

/// A refresh at the token endpoint, as received.
#[derive(Debug, Clone, Default)]
pub struct TokenRefresh {
    pub refresh_token: String,
    pub client_id: String,
    pub resource: Option<String>,
}

/// An access token's user and the resource it was issued for.
#[derive(Debug, Clone)]
pub struct AccessTokenGrant {
    pub principal: crate::domain::UserPrincipal,
    pub resource: String,
}

/// A connected app: one approved authorization and its refresh tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OAuthGrantId(i32);

impl OAuthGrantId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct OAuthGrant {
    pub id: OAuthGrantId,
    pub client_id: String,
    pub client_name: String,
    pub redirect_uri: String,
    pub created_at: OffsetDateTime,
    pub last_used_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_uris_are_https_loopback_http_or_known_app_schemes() {
        for (valid, target) in [
            (
                "https://claude.ai/api/mcp/auth_callback",
                RedirectTarget::Web,
            ),
            ("http://localhost/callback", RedirectTarget::Loopback),
            ("http://127.0.0.1:33418/", RedirectTarget::Loopback),
            ("http://[::1]:8080/cb", RedirectTarget::Loopback),
            (
                "cursor://anysphere.cursor-mcp/oauth/callback",
                RedirectTarget::PrivateUseScheme,
            ),
        ] {
            let uri = RedirectUri::parse(valid).unwrap_or_else(|| panic!("{valid}"));
            assert_eq!(uri.target(), target, "{valid}");
        }
        for invalid in [
            "http://example.com/callback",
            "http://127.0.0.2/callback",
            "https://claude.ai/cb#fragment",
            "https://user:pass@claude.ai/cb",
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
            "JavaScript://%0aalert(1)",
            "mailto:someone@example.com",
            "ms-settings:privacy-webcam",
            "com.example.app:/oauth2redirect",
            "cursor:/no-host",
            "/relative",
        ] {
            assert!(RedirectUri::parse(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn loopback_redirects_match_on_any_port_and_others_exactly() {
        let loopback = RedirectUri::parse("http://localhost/callback").unwrap();
        assert!(loopback.matches("http://localhost/callback"));
        assert!(loopback.matches("http://localhost:53682/callback"));
        assert!(!loopback.matches("http://127.0.0.1:53682/callback"));
        assert!(!loopback.matches("http://localhost:53682/other"));
        assert!(!loopback.matches("http://localhost:53682/callback?x=1"));
        assert!(!loopback.matches("https://localhost:53682/callback"));

        let with_port = RedirectUri::parse("http://127.0.0.1:33418/").unwrap();
        assert!(with_port.matches("http://127.0.0.1:40000/"));
        assert!(with_port.matches("http://127.0.0.1/"));
        assert!(!with_port.matches("http://127.0.0.1:80/"));

        let hosted = RedirectUri::parse("https://claude.ai/api/mcp/auth_callback").unwrap();
        assert!(hosted.matches("https://claude.ai/api/mcp/auth_callback"));
        assert!(!hosted.matches("https://claude.ai:8443/api/mcp/auth_callback"));
        assert!(!hosted.matches("https://claude.ai/api/mcp/auth_callback/"));
        assert!(!hosted.matches("https://CLAUDE.ai/api/mcp/auth_callback"));

        let custom = RedirectUri::parse("cursor://anysphere.cursor-mcp/oauth/callback").unwrap();
        assert!(custom.matches("cursor://anysphere.cursor-mcp/oauth/callback"));
        assert!(!custom.matches("cursor://anysphere.cursor-mcp:1/oauth/callback"));
    }

    #[test]
    fn consent_names_where_a_redirect_goes() {
        let display = |uri: &str| RedirectUri::parse(uri).unwrap().display_destination();
        assert_eq!(
            display("https://claude.ai/api/mcp/auth_callback"),
            "claude.ai"
        );
        assert_eq!(display("http://127.0.0.1:33418/"), "127.0.0.1:33418");
        assert_eq!(display("http://localhost/callback"), "localhost");
        assert_eq!(
            display("cursor://anysphere.cursor-mcp/oauth/callback"),
            "cursor://"
        );
    }

    #[test]
    fn pkce_verifies_the_rfc_7636_example() {
        let challenge =
            PkceChallenge::parse_s256("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM").unwrap();
        assert!(challenge.verify("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert!(!challenge.verify("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXl"));
        assert!(!challenge.verify("too-short"));
        assert!(PkceChallenge::parse_s256("plain-challenge").is_none());
        assert!(
            PkceChallenge::parse_s256("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM=").is_none()
        );
    }

    #[test]
    fn secrets_parse_only_as_their_own_kind() {
        let secret = OAuthSecret::generate(OAuthSecretKind::AccessToken);
        let parsed = OAuthSecret::parse(OAuthSecretKind::AccessToken, secret.as_str()).unwrap();
        assert_eq!(parsed.hash(), secret.hash());
        assert!(OAuthSecret::parse(OAuthSecretKind::RefreshToken, secret.as_str()).is_none());
        assert!(OAuthSecret::parse(
            OAuthSecretKind::AccessToken,
            &secret.as_str().to_uppercase()
        )
        .is_none());
        assert!(!format!("{secret:?}").contains(secret.as_str()));
    }

    #[test]
    fn client_ids_are_canonical_https_urls_with_a_path() {
        for valid in [
            "https://claude.ai/oauth/claude-code-client-metadata",
            "https://vscode.dev/oauth/client-metadata.json",
            "https://chatgpt.com/oauth/client.json",
        ] {
            assert!(ClientIdUrl::parse(valid).is_some(), "{valid}");
        }
        for invalid in [
            "http://claude.ai/oauth/client",
            "https://claude.ai",
            "https://claude.ai/",
            "https://claude.ai/oauth/../client",
            "https://claude.ai/oauth/./client",
            "https://claude.ai/client#x",
            "https://user@claude.ai/client",
            "https://CLAUDE.ai/client",
            "not a url",
        ] {
            assert!(ClientIdUrl::parse(invalid).is_none(), "{invalid}");
        }
    }

    fn document(json: &str) -> ClientMetadataDocument {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_real_client_documents_validate() {
        for json in [
            r#"{"client_id":"https://claude.ai/oauth/claude-code-client-metadata","client_name":"Claude Code","client_uri":"https://claude.ai","redirect_uris":["http://localhost/callback","http://127.0.0.1/callback"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none"}"#,
            r#"{"client_id":"https://claude.ai/oauth/mcp-oauth-client-metadata","client_name":"Claude","client_uri":"https://claude.ai","redirect_uris":["https://claude.ai/api/mcp/auth_callback"],"grant_types":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:jwt-bearer"],"response_types":["code"],"token_endpoint_auth_method":"none"}"#,
            r#"{"client_name":"Visual Studio Code","logo_uri":"https://code.visualstudio.com/assets/branding/code-stable.png","grant_types":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:device_code"],"response_types":["code"],"token_endpoint_auth_method":"none","application_type":"native","client_id":"https://vscode.dev/oauth/client-metadata.json","client_uri":"https://vscode.dev/product","redirect_uris":["http://127.0.0.1:33418/","https://vscode.dev/redirect"]}"#,
            r#"{"client_id":"https://chatgpt.com/oauth/client.json","client_uri":"https://chatgpt.com/","redirect_uris":["https://chatgpt.com/connector_platform_oauth_redirect"],"token_endpoint_auth_method":"private_key_jwt","token_endpoint_auth_methods_supported":["none","private_key_jwt"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"client_name":"ChatGPT","logo_uri":"https://persistent.oaistatic.com/sonic/misc/openai-logo.png","token_endpoint_auth_signing_alg":"RS256","jwks_uri":"https://chatgpt.com/oauth/jwks.json"}"#,
        ] {
            let document = document(json);
            let client_id = ClientIdUrl::parse(&document.client_id).unwrap();
            let client = OAuthClient::from_metadata_document(&client_id, &document)
                .unwrap_or_else(|error| panic!("{}: {error}", document.client_id));
            assert_eq!(client.kind, OAuthClientKind::MetadataDocument);
            assert_eq!(client.verified_host().as_deref(), Some(client_id.host()));
        }
    }

    #[test]
    fn documents_must_name_their_url_and_accept_public_clients() {
        let client_id = ClientIdUrl::parse("https://claude.ai/oauth/client").unwrap();
        let rejected =
            |json: &str| OAuthClient::from_metadata_document(&client_id, &document(json)).is_err();
        assert!(rejected(
            r#"{"client_id":"https://evil.example/oauth/client","redirect_uris":["https://claude.ai/cb"]}"#
        ));
        assert!(rejected(
            r#"{"client_id":"https://claude.ai/oauth/client","redirect_uris":["https://claude.ai/cb"],"token_endpoint_auth_method":"private_key_jwt"}"#
        ));
        assert!(rejected(
            r#"{"client_id":"https://claude.ai/oauth/client","redirect_uris":["https://claude.ai/cb"],"token_endpoint_auth_methods_supported":["private_key_jwt"]}"#
        ));
        assert!(rejected(
            r#"{"client_id":"https://claude.ai/oauth/client","redirect_uris":[]}"#
        ));
        assert!(rejected(
            r#"{"client_id":"https://claude.ai/oauth/client","redirect_uris":["http://evil.example/cb"]}"#
        ));
        assert!(rejected(
            r#"{"client_id":"https://claude.ai/oauth/client","client_name":"line\nbreak","redirect_uris":["https://claude.ai/cb"]}"#
        ));

        let unnamed = OAuthClient::from_metadata_document(
            &client_id,
            &document(r#"{"client_id":"https://claude.ai/oauth/client","redirect_uris":["https://claude.ai/cb"]}"#),
        )
        .unwrap();
        assert_eq!(unnamed.name, "claude.ai");
    }
}
