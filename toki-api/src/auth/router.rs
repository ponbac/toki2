use axum::{
    extract::Query,
    response::{IntoResponse, Redirect},
    routing::{delete, get, post},
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
        .route("/me/avatar", get(self::get::avatar))
        .route("/me/avatar", post(self::post::upload_avatar))
        .route("/me/avatar", delete(self::post::delete_avatar))
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
    use axum::{extract::{Multipart, State}, http::StatusCode};

    use crate::auth::backend::AuthSession;
    use crate::repositories::user_repo::UserRepository;

    use super::*;

    const MAX_AVATAR_SIZE: usize = 1 * 1024 * 1024; // 1MiB

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

    pub async fn upload_avatar(
        auth_session: AuthSession,
        State(app_state): State<AppState>,
        mut multipart: Multipart,
    ) -> Result<StatusCode, StatusCode> {
        let user = auth_session
            .user
            .as_ref()
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let mut avatar_bytes: Option<Vec<u8>> = None;
        let mut mime_type: Option<String> = None;

        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| StatusCode::BAD_REQUEST)?
        {
            if field.name() != Some("avatar") {
                continue;
            }

            let content_type = field
                .content_type()
                .map(|ct| ct.to_string())
                .unwrap_or_else(|| "image/png".to_string());

            let bytes = field
                .bytes()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            if bytes.len() > MAX_AVATAR_SIZE {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }

            avatar_bytes = Some(bytes.to_vec());
            mime_type = Some(content_type);
            break;
        }

        let image = avatar_bytes.ok_or(StatusCode::BAD_REQUEST)?;
        let mime_type = mime_type.unwrap_or_else(|| "image/png".to_string());

        if !mime_type.starts_with("image/") {
            return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }

        app_state
            .user_repo
            .set_user_avatar(user.id, image, mime_type)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(StatusCode::NO_CONTENT)
    }

    pub async fn delete_avatar(
        auth_session: AuthSession,
        State(app_state): State<AppState>,
    ) -> Result<StatusCode, StatusCode> {
        let user = auth_session
            .user
            .as_ref()
            .ok_or(StatusCode::UNAUTHORIZED)?;

        app_state
            .user_repo
            .clear_user_avatar(user.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(StatusCode::NO_CONTENT)
    }
}

mod get {
    use axum::{
        extract::State,
        http::{header, HeaderValue, StatusCode},
        response::Response,
        Json,
    };
    use tracing::instrument;

    use crate::{
        auth::backend::{AuthSession, Credentials},
        domain::User,
        repositories::user_repo::UserRepository,
        AppState,
    };

    use super::*;

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MeResponse {
        #[serde(flatten)]
        pub user: User,
        pub has_avatar: bool,
    }

    pub async fn me(
        auth_session: AuthSession,
        State(app_state): State<AppState>,
    ) -> Result<Json<MeResponse>, StatusCode> {
        let user = match auth_session.user {
            Some(user) => user,
            None => return Err(StatusCode::UNAUTHORIZED),
        };

        let has_avatar = app_state
            .user_repo
            .has_user_avatar(user.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(MeResponse { user, has_avatar }))
    }

    pub async fn avatar(
        auth_session: AuthSession,
        State(app_state): State<AppState>,
    ) -> Result<Response, StatusCode> {
        let user = auth_session
            .user
            .as_ref()
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let avatar = app_state
            .user_repo
            .get_user_avatar(user.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let Some(avatar) = avatar else {
            return Err(StatusCode::NOT_FOUND);
        };

        let mut response = Response::new(axum::body::Body::from(avatar.image));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&avatar.mime_type)
                .unwrap_or(HeaderValue::from_static("image/png")),
        );

        Ok(response)
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
