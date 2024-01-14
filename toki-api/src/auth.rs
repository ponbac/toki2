use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_login::tower_sessions::Session;
use serde::Deserialize;

pub const NEXT_URL_KEY: &str = "auth.next-url";

// This allows us to extract the "next" field from the query string. We use this
// to redirect after log in.
#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

pub fn router() -> Router<()> {
    Router::new()
        .route("/login", post(self::post::login))
        .route("/login", get(self::get::login))
        .route("/logout", get(self::get::logout))
}

mod post {
    use crate::{domain::AuthSession, oauth::CSRF_STATE_KEY};

    use super::*;

    pub async fn login(
        auth_session: AuthSession,
        session: Session,
        Form(NextUrl { next }): Form<NextUrl>,
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
}

mod get {
    use axum::response::Html;

    use crate::domain::AuthSession;

    use super::*;

    pub async fn login(Query(NextUrl { next }): Query<NextUrl>) -> Html<String> {
        format!(
            r#"
            <html>
                <head>
                    <title>Login</title>
                </head>
                <body>
                    <form action="/login" method="post">
                        <input type="hidden" name="next" value="{next}" />
                        <button type="submit">Login</button>
                    </form>
                </body>
            </html>
            "#,
            next = next.unwrap_or_else(|| "/".to_string())
        )
        .into()
    }

    pub async fn logout(mut auth_session: AuthSession) -> impl IntoResponse {
        match auth_session.logout().await {
            Ok(_) => Redirect::to("/login").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
