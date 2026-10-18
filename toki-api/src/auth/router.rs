use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_login::tower_sessions::Session;
use oauth2::CsrfToken;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use url::Url;

use crate::{app_state::AppState, auth::AuthSession};

const LOGIN_CONTINUATION_KEY: &str = "auth.login-continuation";
const CSRF_STATE_KEY: &str = "oauth.csrf-state";
const SIGNED_IN_AT_KEY: &str = "auth.signed-in-at";
/// The only API path a sign-in may continue to.
const AUTHORIZE_PATH_PREFIX: &str = "/oauth/authorize?";

/// Where the browser goes once sign-in completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginContinuation {
    /// A path of the web app, or the TUI's loopback callback.
    App(Option<String>),
    /// An authorization request to Toki's OAuth server: its path and query,
    /// starting `/oauth/authorize?`.
    Authorize(String),
}

/// Starts Entra sign-in. The CSRF state and continuation it stores replace
/// those of any sign-in started earlier in the session, so an abandoned
/// sign-in cannot redirect a later one.
pub async fn start_sign_in(
    auth_session: &AuthSession,
    continuation: LoginContinuation,
) -> Response {
    let (auth_url, csrf_state) = auth_session.backend.authorize_url();

    let stored = async {
        auth_session
            .session
            .insert(CSRF_STATE_KEY, csrf_state.secret())
            .await?;
        auth_session
            .session
            .insert(LOGIN_CONTINUATION_KEY, continuation)
            .await
    };
    if let Err(error) = stored.await {
        tracing::error!("Failed to store the sign-in state: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Redirect::to(auth_url.as_str()).into_response()
}

/// Whether this session's user completed an Entra sign-in within `within`.
/// Sessions outlive the sign-in that created them, so actions that must not
/// outlast a disabled Entra account ask for a recent one.
pub async fn signed_in_within(session: &Session, within: time::Duration) -> bool {
    let Ok(Some(signed_in_at)) = session.get::<i64>(SIGNED_IN_AT_KEY).await else {
        return false;
    };
    let elapsed = OffsetDateTime::now_utc().unix_timestamp() - signed_in_at;
    (0..within.whole_seconds()).contains(&elapsed)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(self::get::me))
        .route("/login", post(self::post::login))
        .route("/logout", get(self::post::logout))
        .route("/oauth/callback", get(self::get::callback))
}

#[derive(Debug, Deserialize)]
struct NextUrl {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuthzResp {
    code: String,
    state: CsrfToken,
}

mod post {
    use super::*;

    pub async fn login(
        auth_session: AuthSession,
        Query(NextUrl { next }): Query<NextUrl>,
    ) -> Response {
        start_sign_in(&auth_session, LoginContinuation::App(next)).await
    }

    pub async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
        match auth_session.logout().await {
            Ok(_) => Redirect::to("/login").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

mod get {
    use axum::{extract::State, Json};

    use crate::{auth::backend::Credentials, domain::User, observability::record_span_field};

    use super::*;

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct MeResponse {
        #[serde(flatten)]
        user: User,
        avatar_url: Option<String>,
    }

    pub async fn me(
        auth_session: AuthSession,
        State(app_state): State<AppState>,
    ) -> Result<Json<MeResponse>, StatusCode> {
        let user = match auth_session.user {
            Some(user) => user,
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        let avatar_url = app_state
            .avatar_service
            .get_avatar_url(&user.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(MeResponse { user, avatar_url }))
    }

    pub async fn callback(
        mut auth_session: AuthSession,
        Query(AuthzResp {
            code,
            state: new_state,
        }): Query<AuthzResp>,
        State(app_state): State<AppState>,
    ) -> impl IntoResponse {
        let Ok(Some(old_state)) = auth_session.session.get(CSRF_STATE_KEY).await else {
            tracing::error!("Failed to get CSRF state from session");
            return StatusCode::BAD_REQUEST.into_response();
        };

        let creds = Credentials {
            code,
            old_state,
            new_state,
        };

        let user = match auth_session.authenticate(creds).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                tracing::error!("CSRF state validation failed");
                return (StatusCode::UNAUTHORIZED, "Invalid CSRF state!").into_response();
            }
            Err(e) => {
                tracing::error!("Authentication failed: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        if let Err(e) = auth_session.login(&user).await {
            tracing::error!("Failed to log in user: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        if let Err(e) = auth_session
            .session
            .insert(SIGNED_IN_AT_KEY, OffsetDateTime::now_utc().unix_timestamp())
            .await
        {
            tracing::error!("Failed to record the sign-in time: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        let continuation = auth_session
            .session
            .remove::<LoginContinuation>(LOGIN_CONTINUATION_KEY)
            .await
            .ok()
            .flatten();
        let next_url = match continuation {
            Some(LoginContinuation::Authorize(path)) => {
                return match authorize_continuation_url(&app_state.api_url, &path) {
                    Some(url) => Redirect::to(&url).into_response(),
                    None => StatusCode::BAD_REQUEST.into_response(),
                };
            }
            Some(LoginContinuation::App(Some(next_url))) => next_url,
            Some(LoginContinuation::App(None)) | None => String::new(),
        };
        record_span_field("next_present", !next_url.is_empty());

        let redirect_url = match next_url.as_str() {
            next if is_tui_callback(next) => {
                match tui_callback_redirect_url(&auth_session.session, next).await {
                    Ok(url) => url,
                    Err(status) => return status.into_response(),
                }
            }
            next => app_redirect_url(&app_state.app_url, next),
        };

        Redirect::to(&redirect_url).into_response()
    }
}

/// The API URL an `Authorize` continuation returns to, if `path` is an
/// authorization request path.
fn authorize_continuation_url(api_url: &Url, path: &str) -> Option<String> {
    if !path.starts_with(AUTHORIZE_PATH_PREFIX) {
        return None;
    }
    let url = api_url.join(path).ok()?;
    (url.origin() == api_url.origin()).then(|| url.to_string())
}

/// Where the web app's sign-in returns to: `next` within the app, or the app's
/// start page for anything else, such as another site, so that a sign-in
/// link cannot send a freshly signed-in user elsewhere.
fn app_redirect_url(app_url: &Url, next: &str) -> String {
    match app_url.join(next) {
        Ok(url) if url.origin() == app_url.origin() => url.to_string(),
        Ok(_) => {
            tracing::warn!("Ignoring a sign-in next URL outside the app");
            app_url.to_string()
        }
        Err(error) => {
            tracing::warn!("Ignoring an invalid sign-in next URL: {error}");
            app_url.to_string()
        }
    }
}

async fn tui_callback_redirect_url(
    session: &Session,
    callback_url: &str,
) -> Result<String, StatusCode> {
    // axum-login rotates the session ID at login to prevent fixation attacks.
    // The new ID is only available after persisting the session.
    if let Err(e) = session.save().await {
        tracing::error!("Failed to save session after login: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let Some(session_id) = session.id() else {
        tracing::error!("Session ID missing after save");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    Ok(format!("{callback_url}?session_id={session_id}"))
}

/// Returns true if this next URL is the TUI's local OAuth callback listener.
/// Validates the exact expected format (http://localhost:<port>/callback) rather
/// than accepting any localhost URL, to avoid open-redirect abuse.
fn is_tui_callback(url: &str) -> bool {
    // Must be http://localhost:<port>/callback with nothing after
    let Some(rest) = url.strip_prefix("http://localhost:") else {
        return false;
    };

    matches!(
        rest.split_once('/'),
        Some((port, "callback"))
            if !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit())
                && port.parse::<u16>().is_ok()
    )
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{header, Request},
        Json,
    };
    use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder};
    use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;
    use tower_sessions_moka_store::MokaStore;

    use super::*;
    use crate::{auth::AuthBackend, config::DatabaseSettings};

    const AUTHORIZE: &str = "/oauth/authorize?client_id=https%3A%2F%2Fclaude.ai%2Fc";

    #[test]
    fn an_authorize_continuation_returns_only_to_the_authorization_endpoint() {
        let api_url = Url::parse("https://toki-api.example").unwrap();

        assert_eq!(
            authorize_continuation_url(&api_url, AUTHORIZE).as_deref(),
            Some(format!("https://toki-api.example{AUTHORIZE}").as_str())
        );
        for path in [
            "/oauth/token?x=1",
            "/oauth/authorizex?x=1",
            "/oauth/authorize",
            "//evil.example/oauth/authorize?x=1",
            "https://evil.example/oauth/authorize?x=1",
        ] {
            assert_eq!(authorize_continuation_url(&api_url, path), None, "{path}");
        }
    }

    #[test]
    fn an_app_sign_in_returns_only_within_the_app() {
        let app_url = Url::parse("https://toki.example").unwrap();

        assert_eq!(
            app_redirect_url(&app_url, "/prs?tab=mine"),
            "https://toki.example/prs?tab=mine"
        );
        assert_eq!(app_redirect_url(&app_url, ""), "https://toki.example/");
        for next in [
            "https://evil.example/phish",
            "//evil.example/phish",
            "/\\evil.example/phish",
            "http://toki.example/prs",
        ] {
            assert_eq!(
                app_redirect_url(&app_url, next),
                "https://toki.example/",
                "{next}"
            );
        }
    }

    /// A router with the session layer, the real `/login`, a route that
    /// starts an authorization sign-in, and one that reads the stored state.
    fn app() -> Router {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:password@localhost/toki_unused")
            .unwrap();
        let database = DatabaseSettings {
            username: "postgres".to_string(),
            password: "password".to_string(),
            port: 5432,
            host: "localhost".to_string(),
            database_name: "toki_unused".to_string(),
            require_ssl: false,
        };
        let backend = AuthBackend::new(
            crate::db::traced_pool(pool, &database),
            BasicClient::new(ClientId::new("client".to_string()))
                .set_client_secret(ClientSecret::new("secret".to_string()))
                .set_auth_uri(AuthUrl::new("https://login.example/authorize".to_string()).unwrap())
                .set_token_uri(TokenUrl::new("https://login.example/token".to_string()).unwrap())
                .set_redirect_uri(
                    RedirectUrl::new("https://toki-api.example/oauth/callback".to_string())
                        .unwrap(),
                ),
        );
        let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);

        Router::new()
            .route("/login", post(post::login))
            .route(
                "/authorize-sign-in",
                get(|session: AuthSession| async move {
                    start_sign_in(&session, LoginContinuation::Authorize(AUTHORIZE.into())).await
                }),
            )
            .route(
                "/sign-in-state",
                get(|session: AuthSession| async move {
                    let continuation: Option<LoginContinuation> =
                        session.session.get(LOGIN_CONTINUATION_KEY).await.unwrap();
                    let csrf_state: Option<String> =
                        session.session.get(CSRF_STATE_KEY).await.unwrap();
                    Json((continuation, csrf_state))
                }),
            )
            .layer(AuthManagerLayerBuilder::new(backend, sessions).build())
    }

    async fn send(app: &Router, method: &str, uri: &str, cookie: Option<&str>) -> Response {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        app.clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn sign_in_state(
        app: &Router,
        cookie: &str,
    ) -> (Option<LoginContinuation>, Option<String>) {
        let response = send(app, "GET", "/sign-in-state", Some(cookie)).await;
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn a_web_sign_in_replaces_an_abandoned_authorization_sign_in() {
        let app = app();
        let started = send(&app, "GET", "/authorize-sign-in", None).await;
        assert_eq!(started.status(), StatusCode::SEE_OTHER);
        assert!(started.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .starts_with("https://login.example/authorize?"));
        let cookie = started.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let (continuation, first_csrf) = sign_in_state(&app, &cookie).await;
        assert_eq!(
            continuation,
            Some(LoginContinuation::Authorize(AUTHORIZE.into()))
        );

        let login = send(&app, "POST", "/login?next=/prs", Some(&cookie)).await;

        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        let (continuation, csrf) = sign_in_state(&app, &cookie).await;
        assert_eq!(
            continuation,
            Some(LoginContinuation::App(Some("/prs".to_string())))
        );
        assert!(csrf.is_some());
        assert_ne!(csrf, first_csrf);
    }
}
