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

        // Redirect::to(auth_url.as_str()).into_response()
        auth_url.as_str().to_string()
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

    pub async fn me(auth_session: AuthSession) -> Result<Json<User>, StatusCode> {
        let user = match auth_session.user {
            Some(user) => user,
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        Ok(Json(user))
    }

    #[instrument(name = "auth_callback", skip(auth_session, session, app_state))]
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

        if let Ok(Some(next)) = session.remove::<String>(NEXT_URL_KEY).await {
            let dest = app_state.app_url.join(next.as_str());
            match dest {
                Ok(url) => Redirect::to(url.as_str()).into_response(),
                Err(e) => {
                    tracing::error!("Failed to join next URL with app URL: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        } else {
            Redirect::to(app_state.app_url.as_str()).into_response()
        }
    }
}
