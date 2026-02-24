use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use axum_login::tower_sessions::Session;
use oauth2::CsrfToken;
use serde::Deserialize;

use crate::app_state::AppState;

const NEXT_URL_KEY: &str = "auth.next-url";
const CSRF_STATE_KEY: &str = "oauth.csrf-state";

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
    use crate::auth::backend::AuthSession;

    use super::*;

    pub async fn login(
        auth_session: AuthSession,
        session: Session,
        Query(NextUrl { next }): Query<NextUrl>,
    ) -> impl IntoResponse {
        let (auth_url, csrf_state) = auth_session.backend.authorize_url();

        session
            .insert(CSRF_STATE_KEY, csrf_state.secret())
            .await
            .expect("Serialization should not fail.");
        session
            .insert(NEXT_URL_KEY, next)
            .await
            .expect("Serialization should not fail.");

        Redirect::to(auth_url.as_str()).into_response()
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
    use tracing::instrument;

    use crate::{
        auth::backend::{AuthSession, Credentials},
        domain::User,
    };

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

    #[instrument(name = "auth_callback", skip(auth_session, session))]
    pub async fn callback(
        mut auth_session: AuthSession,
        session: Session,
        Query(AuthzResp {
            code,
            state: new_state,
        }): Query<AuthzResp>,
        State(app_state): State<AppState>,
    ) -> impl IntoResponse {
        let Ok(Some(old_state)) = session.get(CSRF_STATE_KEY).await else {
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

        let next_url = session
            .remove::<String>(NEXT_URL_KEY)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let redirect_url = if is_tui_callback(&next_url) {
            // axum-login's login() calls cycle_id() which sets the session ID to
            // None until the session is persisted. Save explicitly so we can read
            // the newly-assigned ID before building the redirect URL.
            if let Err(e) = session.save().await {
                tracing::error!("Failed to save session after login: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            let session_id = session.id().map(|id| id.to_string()).unwrap_or_default();
            format!("{}?session_id={}", next_url, session_id)
        } else if next_url.is_empty() {
            app_state.app_url.to_string()
        } else {
            match app_state.app_url.join(&next_url) {
                Ok(url) => url.to_string(),
                Err(e) => {
                    tracing::error!("Failed to join next URL with app URL: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        };

        Redirect::to(&redirect_url).into_response()
    }
}

/// Returns true if this next URL is the TUI's local OAuth callback listener.
/// Validates the exact expected format (http://localhost:<port>/callback) rather
/// than accepting any localhost URL, to avoid open-redirect abuse.
fn is_tui_callback(url: &str) -> bool {
    // Must be http://localhost:<digits>/callback with nothing after
    let Some(rest) = url.strip_prefix("http://localhost:") else {
        return false;
    };
    let Some(path_start) = rest.find('/') else {
        return false;
    };
    let port_str = &rest[..path_start];
    let path = &rest[path_start..];
    port_str.chars().all(|c| c.is_ascii_digit()) && path == "/callback"
}
