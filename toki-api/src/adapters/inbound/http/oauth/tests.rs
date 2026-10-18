//! Router tests against a fake [`OAuthService`] and a real session layer.
//! Tests that need a signed-in user use `#[sqlx::test]` for the users row the
//! session backend loads.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::Path,
    http::{
        header::{
            AUTHORIZATION, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, LOCATION,
            ORIGIN, PRAGMA, REFERRER_POLICY, RETRY_AFTER, SET_COOKIE, WWW_AUTHENTICATE,
            X_FRAME_OPTIONS,
        },
        Method, Request, StatusCode,
    },
    response::Response,
    routing::{get, post},
    Json, Router,
};
use axum_login::{
    tower_sessions::SessionManagerLayer, AuthManagerLayer, AuthManagerLayerBuilder, AuthnBackend,
};
use http_body_util::BodyExt;
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
use time::{macros::datetime, OffsetDateTime};
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;
use url::Url;

use super::{
    grants_router,
    register::{MAX_REGISTRATION_BYTES, REGISTRATIONS_PER_WINDOW},
    server_router, OAuthServerConfig,
};
use crate::{
    auth::{AuthBackend, AuthSession, LoginContinuation},
    domain::{
        models::{
            AuthorizationErrorCode, AuthorizationOutcome, AuthorizationParams,
            AuthorizationResponse, ClientRegistration, CodeExchange, IssuedTokens, OAuthClient,
            OAuthClientKind, OAuthGrant, OAuthGrantId, OAuthSecret, OAuthSecretKind, PkceChallenge,
            RedirectUri, RegisteredClient, TokenRefresh, UserId, ValidatedAuthorization,
            ACCESS_TOKEN_LIFETIME,
        },
        ports::inbound::OAuthService,
        AuthorizationError, OAuthError,
    },
};

const ISSUER: &str = "https://toki.example";
const CLIENT_NAME: &str = r#"Evil <b>"App"</b> & Co"#;
const ESCAPED_CLIENT_NAME: &str = "Evil &lt;b&gt;&quot;App&quot;&lt;/b&gt; &amp; Co";
const METADATA_CLIENT: &str = "https://claude.ai/oauth/mcp-oauth-client-metadata";
const REDIRECT_URI: &str = "https://claude.ai/api/mcp/auth_callback";
const ENTRA_AUTHORIZE_URL: &str = "https://login.example/authorize";
const SIGNED_IN_AT_KEY: &str = "auth.signed-in-at";
const LOGIN_CONTINUATION_KEY: &str = "auth.login-continuation";

/// Acts on the values it receives: special client ids, codes and tokens pick
/// an outcome, so each test reads as its request.
#[derive(Default)]
struct FakeService {
    pending: Mutex<Vec<Pending>>,
    exchanges: Mutex<Vec<CodeExchange>>,
    refreshes: Mutex<Vec<TokenRefresh>>,
    revocations: Mutex<Vec<(String, Option<String>)>>,
    registrations: Mutex<Vec<ClientRegistration>>,
    revoked_grants: Mutex<Vec<(UserId, OAuthGrantId)>>,
}

struct Pending {
    handle: String,
    user_id: UserId,
    redirect_uri: String,
    state: Option<String>,
}

fn issued() -> IssuedTokens {
    IssuedTokens {
        access_token: OAuthSecret::generate(OAuthSecretKind::AccessToken),
        refresh_token: OAuthSecret::generate(OAuthSecretKind::RefreshToken),
        expires_in: ACCESS_TOKEN_LIFETIME,
    }
}

/// The error a special code or refresh token stands for, if any.
fn scripted_error(value: &str) -> Option<OAuthError> {
    match value {
        "invalid-request" => Some(OAuthError::InvalidRequest("bad request".into())),
        "invalid-client" => Some(OAuthError::InvalidClient("unknown client".into())),
        "invalid-grant" => Some(OAuthError::InvalidGrant("code expired".into())),
        "unavailable" => Some(OAuthError::ClientMetadataUnavailable(
            "document unavailable".into(),
        )),
        "storage" => Some(OAuthError::Storage("connection reset".into())),
        _ => None,
    }
}

#[async_trait]
impl OAuthService for FakeService {
    async fn validate_authorization(
        &self,
        params: AuthorizationParams,
    ) -> Result<ValidatedAuthorization, AuthorizationError> {
        let client_id = params.client_id.unwrap_or_default();
        if client_id == "unknown" {
            return Err(AuthorizationError::Unredirectable(
                OAuthError::InvalidClient("The client <unknown> is not registered.".into()),
            ));
        }
        let redirect_uri = params.redirect_uri.unwrap_or_default();
        if params.response_type.as_deref() != Some("code") {
            return Err(AuthorizationError::Redirect(AuthorizationResponse {
                redirect_uri,
                state: params.state,
                outcome: AuthorizationOutcome::Error {
                    code: AuthorizationErrorCode::UnsupportedResponseType,
                    description: "Only the code response type is supported.".into(),
                },
            }));
        }
        let registered = RedirectUri::parse(&redirect_uri).expect("test redirect URI");
        let kind = if client_id.starts_with("https://") {
            OAuthClientKind::MetadataDocument
        } else {
            OAuthClientKind::Registered
        };
        Ok(ValidatedAuthorization {
            client: OAuthClient {
                id: client_id,
                name: CLIENT_NAME.to_string(),
                kind,
                redirect_uris: vec![registered.clone()],
            },
            registered_redirect_uri: registered,
            redirect_uri,
            code_challenge: PkceChallenge::parse_s256(
                "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            )
            .unwrap(),
            resource: format!("{ISSUER}/mcp"),
            state: params.state,
        })
    }

    async fn begin_consent(
        &self,
        user_id: &UserId,
        authorization: &ValidatedAuthorization,
    ) -> Result<OAuthSecret, OAuthError> {
        let handle = OAuthSecret::generate(OAuthSecretKind::AuthorizationRequest);
        self.pending.lock().unwrap().push(Pending {
            handle: handle.as_str().to_string(),
            user_id: *user_id,
            redirect_uri: authorization.redirect_uri.clone(),
            state: authorization.state.clone(),
        });
        Ok(handle)
    }

    async fn decide(
        &self,
        user_id: &UserId,
        request: &str,
        allow: bool,
    ) -> Result<AuthorizationResponse, OAuthError> {
        let mut pending = self.pending.lock().unwrap();
        let index = pending
            .iter()
            .position(|pending| pending.handle == request && pending.user_id == *user_id)
            .ok_or(OAuthError::NotFound)?;
        let decided = pending.remove(index);
        let outcome = if allow {
            AuthorizationOutcome::Code(OAuthSecret::generate(OAuthSecretKind::AuthorizationCode))
        } else {
            AuthorizationOutcome::Error {
                code: AuthorizationErrorCode::AccessDenied,
                description: "The user denied the request.".into(),
            }
        };
        Ok(AuthorizationResponse {
            redirect_uri: decided.redirect_uri,
            state: decided.state,
            outcome,
        })
    }

    async fn exchange_code(&self, exchange: CodeExchange) -> Result<IssuedTokens, OAuthError> {
        let error = scripted_error(&exchange.code);
        self.exchanges.lock().unwrap().push(exchange);
        error.map_or_else(|| Ok(issued()), Err)
    }

    async fn refresh(&self, refresh: TokenRefresh) -> Result<IssuedTokens, OAuthError> {
        let error = scripted_error(&refresh.refresh_token);
        self.refreshes.lock().unwrap().push(refresh);
        error.map_or_else(|| Ok(issued()), Err)
    }

    async fn revoke_token(&self, token: &str, client_id: Option<&str>) -> Result<(), OAuthError> {
        self.revocations
            .lock()
            .unwrap()
            .push((token.to_string(), client_id.map(str::to_string)));
        scripted_error(token).map_or(Ok(()), Err)
    }

    async fn register_client(
        &self,
        registration: ClientRegistration,
    ) -> Result<RegisteredClient, OAuthError> {
        self.registrations
            .lock()
            .unwrap()
            .push(registration.clone());
        let redirect_uris = registration.redirect_uris.unwrap_or_default();
        if redirect_uris
            .iter()
            .any(|uri| uri.starts_with("http://evil.example"))
        {
            return Err(OAuthError::InvalidRedirectUri(
                "http redirect URIs must be loopback.".into(),
            ));
        }
        if registration
            .token_endpoint_auth_method
            .is_some_and(|m| m != "none")
        {
            return Err(OAuthError::InvalidClientMetadata(
                "Only public clients can register.".into(),
            ));
        }
        Ok(RegisteredClient {
            client: OAuthClient {
                id: "0123456789abcdef0123456789abcdef".to_string(),
                name: registration.client_name.unwrap_or_else(|| "Unnamed".into()),
                kind: OAuthClientKind::Registered,
                redirect_uris: redirect_uris
                    .iter()
                    .map(|uri| RedirectUri::parse(uri).unwrap())
                    .collect(),
            },
            created_at: datetime!(2026-10-18 12:00 UTC),
        })
    }

    async fn list_grants(&self, _user_id: &UserId) -> Result<Vec<OAuthGrant>, OAuthError> {
        Ok(vec![
            OAuthGrant {
                id: OAuthGrantId::new(2),
                client_id: "0123456789abcdef0123456789abcdef".to_string(),
                client_name: "Cursor".to_string(),
                redirect_uri: "http://127.0.0.1:53682/callback".to_string(),
                created_at: datetime!(2026-10-18 10:00 UTC),
                last_used_at: datetime!(2026-10-18 11:00 UTC),
                expires_at: datetime!(2027-01-16 10:00 UTC),
            },
            OAuthGrant {
                id: OAuthGrantId::new(1),
                client_id: METADATA_CLIENT.to_string(),
                client_name: "Claude".to_string(),
                redirect_uri: REDIRECT_URI.to_string(),
                created_at: datetime!(2026-10-01 08:30 UTC),
                last_used_at: datetime!(2026-10-17 09:15:30 UTC),
                expires_at: datetime!(2026-12-30 08:30 UTC),
            },
        ])
    }

    async fn revoke_grant(
        &self,
        user_id: &UserId,
        grant_id: &OAuthGrantId,
    ) -> Result<(), OAuthError> {
        if grant_id.as_i32() != 1 {
            return Err(OAuthError::NotFound);
        }
        self.revoked_grants
            .lock()
            .unwrap()
            .push((*user_id, *grant_id));
        Ok(())
    }

    async fn remove_expired(&self) -> Result<(), OAuthError> {
        Ok(())
    }
}

fn auth_layer(pool: PgPool) -> AuthManagerLayer<AuthBackend, MokaStore> {
    let backend = AuthBackend::new(
        crate::db::DbPool::from(pool),
        BasicClient::new(ClientId::new("test-client".to_string()))
            .set_client_secret(ClientSecret::new("test-secret".to_string()))
            .set_auth_uri(AuthUrl::new(ENTRA_AUTHORIZE_URL.to_string()).unwrap())
            .set_token_uri(TokenUrl::new("https://login.example/token".to_string()).unwrap())
            .set_redirect_uri(
                RedirectUrl::new("https://toki.example/oauth/callback".to_string()).unwrap(),
            ),
    );
    let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);
    AuthManagerLayerBuilder::new(backend, sessions).build()
}

/// A pool that is never connected, for tests without a signed-in user.
fn unused_pool() -> PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://postgres:password@localhost:1/unused")
        .unwrap()
}

/// Signs the session in as `user_id`, with the Entra sign-in `seconds_ago`.
async fn test_sign_in(
    mut auth_session: AuthSession,
    Path((user_id, seconds_ago)): Path<(i64, i64)>,
) -> StatusCode {
    let user = auth_session
        .backend
        .get_user(&user_id)
        .await
        .unwrap()
        .unwrap();
    auth_session.login(&user).await.unwrap();
    auth_session
        .session
        .insert(
            SIGNED_IN_AT_KEY,
            OffsetDateTime::now_utc().unix_timestamp() - seconds_ago,
        )
        .await
        .unwrap();
    StatusCode::NO_CONTENT
}

async fn test_continuation(auth_session: AuthSession) -> Json<Option<LoginContinuation>> {
    Json(
        auth_session
            .session
            .get(LOGIN_CONTINUATION_KEY)
            .await
            .unwrap(),
    )
}

fn app(service: Arc<FakeService>, pool: PgPool) -> Router {
    let config = OAuthServerConfig {
        issuer: ISSUER.to_string(),
        api_origin: ISSUER.to_string(),
    };
    Router::new()
        .merge(server_router(service.clone(), config))
        .nest("/users/me/oauth-grants", grants_router(service))
        .route("/test/sign-in/{user_id}/{seconds_ago}", post(test_sign_in))
        .route("/test/continuation", get(test_continuation))
        .layer(auth_layer(pool))
}

/// A browser: keeps the session cookie between requests.
struct Browser {
    app: Router,
    cookie: Option<String>,
}

impl Browser {
    fn new(app: Router) -> Self {
        Self { app, cookie: None }
    }

    async fn send(&mut self, mut request: Request<Body>) -> Response {
        if let Some(cookie) = &self.cookie {
            request
                .headers_mut()
                .insert(COOKIE, cookie.parse().unwrap());
        }
        let response = self.app.clone().oneshot(request).await.unwrap();
        if let Some(set_cookie) = response.headers().get(SET_COOKIE) {
            let pair = set_cookie.to_str().unwrap().split(';').next().unwrap();
            self.cookie = Some(pair.to_string());
        }
        response
    }

    async fn get(&mut self, uri: &str) -> Response {
        self.send(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    async fn sign_in(&mut self, user_id: i32, seconds_ago: i64) {
        let response = self
            .send(
                Request::post(format!("/test/sign-in/{user_id}/{seconds_ago}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    async fn continuation(&mut self) -> Option<LoginContinuation> {
        let response = self.get("/test/continuation").await;
        serde_json::from_value(json_body(response).await).unwrap()
    }
}

async fn text_body(response: Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn json_body(response: Response) -> Value {
    serde_json::from_str(&text_body(response).await).unwrap()
}

fn form(uri: &str, body: &str) -> Request<Body> {
    Request::post(uri)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn decision(request: &str, decision: &str) -> Request<Body> {
    let mut request = form(
        "/oauth/authorize",
        &format!("request={request}&decision={decision}"),
    );
    request
        .headers_mut()
        .insert(ORIGIN, ISSUER.parse().unwrap());
    request
        .headers_mut()
        .insert("sec-fetch-site", "same-origin".parse().unwrap());
    request
}

fn authorize_uri(client_id: &str, extra: &str) -> String {
    authorize_uri_to(client_id, REDIRECT_URI, extra)
}

fn authorize_uri_to(client_id: &str, redirect_uri: &str, extra: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair(
            "code_challenge",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        )
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", "client-state")
        .finish();
    format!("/oauth/authorize?{query}{extra}")
}

fn location_pairs(response: &Response) -> (String, Vec<(String, String)>) {
    let location = response.headers()[LOCATION].to_str().unwrap();
    let url = Url::parse(location).unwrap();
    let pairs = url.query_pairs().into_owned().collect();
    let mut base = url.clone();
    base.set_query(None);
    (base.to_string(), pairs)
}

fn pair<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn assert_page_headers(response: &Response) {
    let headers = response.headers();
    assert_eq!(headers[CACHE_CONTROL], "no-store");
    let policy = headers[CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.starts_with("default-src 'none';"), "{policy}");
    assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
    assert!(!policy.contains("script-src"), "{policy}");
    assert_eq!(headers[X_FRAME_OPTIONS], "DENY");
    assert!(headers[CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/html"));
}

async fn insert_user(pool: &PgPool, email: &str) -> i32 {
    sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ($1, 'Test User', '', 'session-hash')
         RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn metadata_lists_the_endpoints_under_the_issuer() {
    let response = Browser::new(app(Arc::default(), unused_pool()))
        .get("/.well-known/oauth-authorization-server")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!({
            "issuer": "https://toki.example",
            "authorization_endpoint": "https://toki.example/oauth/authorize",
            "token_endpoint": "https://toki.example/oauth/token",
            "registration_endpoint": "https://toki.example/oauth/register",
            "revocation_endpoint": "https://toki.example/oauth/revoke",
            "scopes_supported": ["mcp", "offline_access"],
            "response_types_supported": ["code"],
            "response_modes_supported": ["query"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none"],
            "revocation_endpoint_auth_methods_supported": ["none"],
            "client_id_metadata_document_supported": true,
            "authorization_response_iss_parameter_supported": true,
        })
    );
}

#[tokio::test]
async fn an_untrusted_client_gets_an_error_page_and_no_redirect() {
    let response = Browser::new(app(Arc::default(), unused_pool()))
        .get(&authorize_uri("unknown", "&response_type=code"))
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get(LOCATION).is_none());
    assert_page_headers(&response);
    let html = text_body(response).await;
    assert!(html.contains("The client &lt;unknown&gt; is not registered."));
}

#[tokio::test]
async fn repeated_or_oversized_authorize_parameters_get_an_error_page() {
    let mut browser = Browser::new(app(Arc::default(), unused_pool()));

    let repeated = browser
        .get(&authorize_uri(
            METADATA_CLIENT,
            "&response_type=code&state=other",
        ))
        .await;
    assert_eq!(repeated.status(), StatusCode::BAD_REQUEST);
    assert!(repeated.headers().get(LOCATION).is_none());
    assert!(text_body(repeated)
        .await
        .contains("The parameter state is sent more than once."));

    let oversized = browser
        .get(&authorize_uri(
            METADATA_CLIENT,
            &format!("&response_type=code&scope={}", "a".repeat(2049)),
        ))
        .await;
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
    assert!(oversized.headers().get(LOCATION).is_none());
    assert!(text_body(oversized)
        .await
        .contains("The parameter scope is too long."));
}

#[tokio::test]
async fn a_request_error_for_a_web_app_is_shown_with_a_link_back_not_redirected() {
    let response = Browser::new(app(Arc::default(), unused_pool()))
        .get(&authorize_uri(METADATA_CLIENT, "&response_type=token"))
        .await;

    // Anyone can register a web redirect URI, so a request that fails before
    // the user did anything must not send the browser there by itself.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(response.headers().get(LOCATION).is_none());
    assert_page_headers(&response);
    let html = text_body(response).await;
    assert!(html.contains("unsupported_response_type: Only the code response type is supported."));
    let href = html
        .split(r#"<a class="return" href=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a link back to the app")
        .replace("&amp;", "&");
    let url = Url::parse(&href).unwrap();
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(&href[..REDIRECT_URI.len()], REDIRECT_URI);
    assert_eq!(pair(&pairs, "error"), Some("unsupported_response_type"));
    assert_eq!(pair(&pairs, "state"), Some("client-state"));
    assert_eq!(pair(&pairs, "iss"), Some(ISSUER));
    assert!(html.contains(">Return to claude.ai</a>"));
}

#[tokio::test]
async fn a_request_error_for_an_app_on_this_computer_redirects_with_state_and_issuer() {
    let loopback = "http://127.0.0.1:53682/callback";
    let response = Browser::new(app(Arc::default(), unused_pool()))
        .get(&authorize_uri_to(
            METADATA_CLIENT,
            loopback,
            "&response_type=token",
        ))
        .await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let (base, pairs) = location_pairs(&response);
    assert_eq!(base, loopback);
    assert_eq!(pair(&pairs, "error"), Some("unsupported_response_type"));
    assert_eq!(
        pair(&pairs, "error_description"),
        Some("Only the code response type is supported.")
    );
    assert_eq!(pair(&pairs, "state"), Some("client-state"));
    assert_eq!(pair(&pairs, "iss"), Some(ISSUER));
}

#[tokio::test]
async fn authorizing_without_a_session_starts_sign_in_and_returns_here() {
    let mut browser = Browser::new(app(Arc::default(), unused_pool()));
    let uri = authorize_uri(METADATA_CLIENT, "&response_type=code");

    let response = browser.get(&uri).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[LOCATION].to_str().unwrap();
    assert!(location.starts_with(&format!("{ENTRA_AUTHORIZE_URL}?")));
    assert_eq!(
        browser.continuation().await,
        Some(LoginContinuation::Authorize(uri))
    );
}

#[sqlx::test]
async fn a_stale_sign_in_signs_in_again_before_consent(pool: PgPool) {
    let user_id = insert_user(&pool, "ada@example.com").await;
    let service = Arc::new(FakeService::default());
    let mut browser = Browser::new(app(service.clone(), pool));
    browser.sign_in(user_id, 16 * 60).await;
    let uri = authorize_uri(METADATA_CLIENT, "&response_type=code");

    let response = browser.get(&uri).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()[LOCATION].to_str().unwrap();
    assert!(location.starts_with(&format!("{ENTRA_AUTHORIZE_URL}?")));
    assert_eq!(
        browser.continuation().await,
        Some(LoginContinuation::Authorize(uri))
    );
    assert!(service.pending.lock().unwrap().is_empty());
}

#[sqlx::test]
async fn consent_after_a_recent_sign_in_redirects_with_a_code_once(pool: PgPool) {
    let user_id = insert_user(&pool, "ada@example.com").await;
    let service = Arc::new(FakeService::default());
    let mut browser = Browser::new(app(service.clone(), pool));
    browser.sign_in(user_id, 60).await;

    let response = browser
        .get(&authorize_uri(METADATA_CLIENT, "&response_type=code"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_page_headers(&response);
    assert_eq!(response.headers()[REFERRER_POLICY], "same-origin");
    let html = text_body(response).await;
    let handle = service.pending.lock().unwrap()[0].handle.clone();
    assert_eq!(service.pending.lock().unwrap()[0].user_id.as_i32(), user_id);
    assert!(html.contains(&format!("Connect {ESCAPED_CLIENT_NAME} to Toki")));
    assert!(!html.contains(CLIENT_NAME));
    assert!(html.contains("Signed in as <strong>ada@example.com</strong>"));
    assert!(html.contains("Identified by <strong>claude.ai</strong>"));
    assert!(html.contains(&format!(r#"name="request" value="{handle}""#)));

    let response = browser.send(decision(&handle, "allow")).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let (base, pairs) = location_pairs(&response);
    assert_eq!(base, REDIRECT_URI);
    let code = pair(&pairs, "code").unwrap();
    assert!(OAuthSecret::parse(OAuthSecretKind::AuthorizationCode, code).is_some());
    assert_eq!(pair(&pairs, "state"), Some("client-state"));
    assert_eq!(pair(&pairs, "iss"), Some(ISSUER));
    assert_eq!(pair(&pairs, "error"), None);

    let again = browser.send(decision(&handle, "allow")).await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    assert!(again.headers().get(LOCATION).is_none());
    assert!(text_body(again)
        .await
        .contains("expired or was already answered"));
}

#[sqlx::test]
async fn denying_consent_redirects_with_access_denied(pool: PgPool) {
    let user_id = insert_user(&pool, "ada@example.com").await;
    let service = Arc::new(FakeService::default());
    let mut browser = Browser::new(app(service.clone(), pool));
    browser.sign_in(user_id, 0).await;
    let response = browser
        .get(&authorize_uri(
            "0123456789abcdef0123456789abcdef",
            "&response_type=code",
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(text_body(response).await.contains("can't verify"));
    let handle = service.pending.lock().unwrap()[0].handle.clone();

    let response = browser.send(decision(&handle, "deny")).await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let (_, pairs) = location_pairs(&response);
    assert_eq!(pair(&pairs, "error"), Some("access_denied"));
    assert_eq!(pair(&pairs, "code"), None);
    assert_eq!(pair(&pairs, "state"), Some("client-state"));
    assert_eq!(pair(&pairs, "iss"), Some(ISSUER));
}

#[sqlx::test]
async fn a_decision_needs_a_known_choice(pool: PgPool) {
    let user_id = insert_user(&pool, "ada@example.com").await;
    let mut browser = Browser::new(app(Arc::default(), pool));
    browser.sign_in(user_id, 0).await;

    let response = browser.send(decision("tmcp_ar_x", "maybe")).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(text_body(response).await.contains("Choose Allow or Deny"));
}

#[tokio::test]
async fn cross_origin_or_signed_out_decisions_are_refused() {
    let mut browser = Browser::new(app(Arc::default(), unused_pool()));

    let mut cross_origin = decision("tmcp_ar_x", "allow");
    cross_origin
        .headers_mut()
        .insert(ORIGIN, "https://evil.example".parse().unwrap());
    let response = browser.send(cross_origin).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_page_headers(&response);

    let mut cross_site = decision("tmcp_ar_x", "allow");
    cross_site
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert_eq!(
        browser.send(cross_site).await.status(),
        StatusCode::FORBIDDEN
    );

    let signed_out = browser.send(decision("tmcp_ar_x", "allow")).await;
    assert_eq!(signed_out.status(), StatusCode::UNAUTHORIZED);
    assert!(text_body(signed_out)
        .await
        .contains("Start connecting again from the app."));
}

async fn post_token(app: &Router, request: Request<Body>) -> (Response, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[PRAGMA], "no-cache");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (Response::from_parts(parts, Body::empty()), json)
}

const CODE_EXCHANGE: &str = "grant_type=authorization_code&code=tmcp_ac_code\
    &redirect_uri=https%3A%2F%2Fclaude.ai%2Fapi%2Fmcp%2Fauth_callback\
    &client_id=https%3A%2F%2Fclaude.ai%2Foauth%2Fmcp-oauth-client-metadata\
    &code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

#[tokio::test]
async fn the_token_endpoint_exchanges_codes_and_refreshes() {
    let service = Arc::new(FakeService::default());
    let app = app(service.clone(), unused_pool());

    let (response, body) = post_token(
        &app,
        form(
            "/oauth/token",
            &format!("{CODE_EXCHANGE}&resource=https%3A%2F%2Ftoki.example%2Fmcp"),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["expires_in"], 3600);
    assert_eq!(body["scope"], "mcp");
    assert!(body["access_token"]
        .as_str()
        .unwrap()
        .starts_with("tmcp_at_"));
    assert!(body["refresh_token"]
        .as_str()
        .unwrap()
        .starts_with("tmcp_rt_"));
    {
        let exchanges = service.exchanges.lock().unwrap();
        let exchange = &exchanges[0];
        assert_eq!(exchange.code, "tmcp_ac_code");
        assert_eq!(exchange.redirect_uri, REDIRECT_URI);
        assert_eq!(exchange.client_id, METADATA_CLIENT);
        assert_eq!(
            exchange.code_verifier,
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        );
        assert_eq!(
            exchange.resource.as_deref(),
            Some("https://toki.example/mcp")
        );
    }

    let (response, body) = post_token(
        &app,
        form(
            "/oauth/token",
            "grant_type=refresh_token&refresh_token=tmcp_rt_old&client_id=abc&scope=other",
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body["scope"], "mcp");
    let refreshes = service.refreshes.lock().unwrap();
    assert_eq!(refreshes[0].refresh_token, "tmcp_rt_old");
    assert_eq!(refreshes[0].client_id, "abc");
    assert_eq!(refreshes[0].resource, None);
}

#[tokio::test]
async fn token_errors_map_to_oauth_error_codes() {
    let app = app(Arc::default(), unused_pool());

    for (code, status, error) in [
        (
            "invalid-request",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        ("invalid-client", StatusCode::UNAUTHORIZED, "invalid_client"),
        ("invalid-grant", StatusCode::BAD_REQUEST, "invalid_grant"),
        ("unavailable", StatusCode::BAD_REQUEST, "invalid_request"),
        ("storage", StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
    ] {
        let body = CODE_EXCHANGE.replace("tmcp_ac_code", code);
        let (response, json) = post_token(&app, form("/oauth/token", &body)).await;
        assert_eq!(response.status(), status, "{code}");
        assert_eq!(json["error"], error, "{code}");
        assert!(json["error_description"].is_string(), "{code}");
        assert!(!json.to_string().contains("connection reset"), "{code}");

        let refresh = format!("grant_type=refresh_token&refresh_token={code}&client_id=abc");
        let (response, json) = post_token(&app, form("/oauth/token", &refresh)).await;
        assert_eq!(response.status(), status, "refresh {code}");
        assert_eq!(json["error"], error, "refresh {code}");
    }

    for (body, status, error) in [
        (
            "grant_type=password&username=a",
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
        ),
        ("code=x", StatusCode::BAD_REQUEST, "invalid_request"),
        (
            "grant_type=authorization_code&code=x&redirect_uri=y&code_verifier=z",
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
        (
            "grant_type=authorization_code&client_id=x&code=x",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            "grant_type=refresh_token&client_id=x",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
        (
            "grant_type=refresh_token&refresh_token=x",
            StatusCode::UNAUTHORIZED,
            "invalid_client",
        ),
        (
            "grant_type=refresh_token&refresh_token=x&refresh_token=y&client_id=x",
            StatusCode::BAD_REQUEST,
            "invalid_request",
        ),
    ] {
        let (response, json) = post_token(&app, form("/oauth/token", body)).await;
        assert_eq!(response.status(), status, "{body}");
        assert_eq!(json["error"], error, "{body}");
    }

    let json_body = Request::post("/oauth/token")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"grant_type":"refresh_token"}"#))
        .unwrap();
    let (response, json) = post_token(&app, json_body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "invalid_request");

    let too_large = format!("{CODE_EXCHANGE}&pad={}", "&".repeat(16 * 1024));
    let (response, json) = post_token(&app, form("/oauth/token", &too_large)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "invalid_request");
}

#[tokio::test]
async fn clients_that_try_to_authenticate_are_refused() {
    let service = Arc::new(FakeService::default());
    let app = app(service.clone(), unused_pool());

    let mut basic = form("/oauth/token", CODE_EXCHANGE);
    basic
        .headers_mut()
        .insert(AUTHORIZATION, "Basic Y2xpZW50OnNlY3JldA==".parse().unwrap());
    let (response, json) = post_token(&app, basic).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"], "invalid_client");
    assert!(response.headers()[WWW_AUTHENTICATE]
        .to_str()
        .unwrap()
        .starts_with("Basic"));

    let mut bearer = form("/oauth/token", CODE_EXCHANGE);
    bearer
        .headers_mut()
        .insert(AUTHORIZATION, "Bearer abc".parse().unwrap());
    let (response, json) = post_token(&app, bearer).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"], "invalid_client");
    assert!(response.headers().get(WWW_AUTHENTICATE).is_none());

    for credential in [
        "client_secret=s",
        "client_assertion=eyJ",
        "client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer",
    ] {
        let body = format!("{CODE_EXCHANGE}&{credential}");
        let (response, json) = post_token(&app, form("/oauth/token", &body)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{credential}");
        assert_eq!(json["error"], "invalid_client", "{credential}");

        let body = format!("token=tmcp_rt_x&{credential}");
        let (response, _) = post_token(&app, form("/oauth/revoke", &body)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{credential}");
    }

    // Public clients that send an empty secret are not authenticating.
    let (response, _) = post_token(
        &app,
        form("/oauth/token", &format!("{CODE_EXCHANGE}&client_secret=")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(service.exchanges.lock().unwrap().len(), 1);
    assert!(service.revocations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn revocation_succeeds_quietly_and_reports_storage_failures() {
    let service = Arc::new(FakeService::default());
    let app = app(service.clone(), unused_pool());

    let (response, body) = post_token(
        &app,
        form(
            "/oauth/revoke",
            "token=tmcp_rt_x&token_type_hint=refresh_token&client_id=abc",
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body, Value::Null);

    let (response, _) = post_token(&app, form("/oauth/revoke", "token=tmcp_at_y")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        *service.revocations.lock().unwrap(),
        [
            ("tmcp_rt_x".to_string(), Some("abc".to_string())),
            ("tmcp_at_y".to_string(), None),
        ]
    );

    let (response, body) = post_token(&app, form("/oauth/revoke", "token=storage")).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "server_error");

    let (response, body) = post_token(&app, form("/oauth/revoke", "client_id=abc")).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_request");
}

fn register_request(body: &str) -> Request<Body> {
    Request::post("/oauth/register")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn registration_returns_the_public_client() {
    let service = Arc::new(FakeService::default());
    let app = app(service.clone(), unused_pool());

    let response = app
        .clone()
        .oneshot(register_request(
            r#"{"client_name":"Cursor","redirect_uris":["cursor://anysphere.cursor-mcp/oauth/callback","http://127.0.0.1/callback"],"grant_types":["authorization_code","refresh_token"],"response_types":["code"],"token_endpoint_auth_method":"none","logo_uri":"https://cursor.com/logo.png"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(
        json_body(response).await,
        json!({
            "client_id": "0123456789abcdef0123456789abcdef",
            "client_id_issued_at": datetime!(2026-10-18 12:00 UTC).unix_timestamp(),
            "client_name": "Cursor",
            "redirect_uris": [
                "cursor://anysphere.cursor-mcp/oauth/callback",
                "http://127.0.0.1/callback"
            ],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })
    );
    let registrations = service.registrations.lock().unwrap();
    assert_eq!(
        registrations[0].grant_types.as_deref(),
        Some(
            &[
                "authorization_code".to_string(),
                "refresh_token".to_string()
            ][..]
        )
    );
    assert_eq!(
        registrations[0].response_types.as_deref(),
        Some(&["code".to_string()][..])
    );
    assert_eq!(
        registrations[0].token_endpoint_auth_method.as_deref(),
        Some("none")
    );
}

#[tokio::test]
async fn invalid_registrations_name_what_is_wrong() {
    let app = app(Arc::default(), unused_pool());

    for (body, error) in [
        (
            r#"{"redirect_uris":["http://evil.example/cb"]}"#,
            "invalid_redirect_uri",
        ),
        (
            r#"{"redirect_uris":["https://app.example/cb"],"token_endpoint_auth_method":"client_secret_basic"}"#,
            "invalid_client_metadata",
        ),
        ("not json", "invalid_client_metadata"),
        (
            r#"{"redirect_uris":"https://app.example/cb"}"#,
            "invalid_client_metadata",
        ),
    ] {
        let response = app.clone().oneshot(register_request(body)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(json_body(response).await["error"], error, "{body}");
    }

    // Unused registrations are kept for 30 days, so each must stay small.
    let too_large = format!(
        r#"{{"redirect_uris":["https://app.example/{}"]}}"#,
        "a".repeat(MAX_REGISTRATION_BYTES)
    );
    let response = app.oneshot(register_request(&too_large)).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(response).await["error"],
        "invalid_client_metadata"
    );
}

#[tokio::test]
async fn registrations_are_limited_per_hour() {
    let service = Arc::new(FakeService::default());
    let app = app(service.clone(), unused_pool());
    let valid = r#"{"redirect_uris":["https://app.example/cb"]}"#;

    // Refused registrations do not use up the limit.
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(register_request(
                r#"{"redirect_uris":["http://evil.example/cb"]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    for _ in 0..REGISTRATIONS_PER_WINDOW {
        let response = app.clone().oneshot(register_request(valid)).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let response = app.clone().oneshot(register_request(valid)).await.unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after: u64 = response.headers()[RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((3590..=3600).contains(&retry_after), "{retry_after}");
    assert_eq!(
        service.registrations.lock().unwrap().len(),
        5 + REGISTRATIONS_PER_WINDOW
    );
}

#[sqlx::test]
async fn grants_are_listed_and_disconnected(pool: PgPool) {
    let user_id = insert_user(&pool, "ada@example.com").await;
    let service = Arc::new(FakeService::default());
    let mut browser = Browser::new(app(service.clone(), pool));

    let anonymous = browser.get("/users/me/oauth-grants").await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    browser.sign_in(user_id, 0).await;
    let response = browser.get("/users/me/oauth-grants").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await,
        json!([
            {
                "id": 2,
                "clientName": "Cursor",
                "clientHost": null,
                "redirectDestination": "127.0.0.1:53682",
                "createdAt": "2026-10-18T10:00:00Z",
                "lastUsedAt": "2026-10-18T11:00:00Z",
                "expiresAt": "2027-01-16T10:00:00Z",
            },
            {
                "id": 1,
                "clientName": "Claude",
                "clientHost": "claude.ai",
                "redirectDestination": "claude.ai",
                "createdAt": "2026-10-01T08:30:00Z",
                "lastUsedAt": "2026-10-17T09:15:30Z",
                "expiresAt": "2026-12-30T08:30:00Z",
            },
        ])
    );

    let delete = |id: i32| {
        Request::builder()
            .method(Method::DELETE)
            .uri(format!("/users/me/oauth-grants/{id}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        browser.send(delete(1)).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        browser.send(delete(9)).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        *service.revoked_grants.lock().unwrap(),
        [(UserId::new(user_id), OAuthGrantId::new(1))]
    );
}
