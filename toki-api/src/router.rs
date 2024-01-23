use axum::{
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    login_required,
    tower_sessions::{Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, TokenUrl};
use sqlx::PgPool;
use time::Duration;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use crate::{
    app_state::AppState,
    auth,
    config::Settings,
    domain::{AuthBackend, AuthSession, RepoConfig},
    routes,
};

pub async fn create(
    connection_pool: PgPool,
    repo_configs: Vec<RepoConfig>,
    config: Settings,
) -> Router<()> {
    let base_app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pull-requests", get(routes::open_pull_requests))
        .route("/repositories", get(routes::get_repositories))
        .route("/cached-pull-requests", get(routes::cached_pull_requests))
        .route("/start-differ", post(routes::start_differ))
        .route("/stop-differ", post(routes::stop_differ))
        .route("/force-update", post(routes::force_update))
        .route("/repositories", post(routes::add_repository))
        .route("/auth", get(auth_test))
        .with_state(AppState::new(connection_pool.clone(), repo_configs).await);

    // If authentication is enabled, wrap the app with the auth middleware
    let app_with_auth = if config.application.disable_auth {
        base_app
    } else {
        let client = BasicClient::new(
            ClientId::new(config.auth.client_id),
            Some(ClientSecret::new(config.auth.client_secret)),
            AuthUrl::new(config.auth.auth_url).expect("Invalid authorization endpoint URL"),
            Some(TokenUrl::new(config.auth.token_url).expect("Invalid token endpoint URL")),
        );
        let session_store = MemoryStore::default();
        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(Duration::days(1)));

        let backend = AuthBackend::new(connection_pool.clone(), client);
        let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

        base_app
            .route_layer(login_required!(AuthBackend, login_url = "/login"))
            .merge(auth::router())
            .layer(auth_layer)
    };

    app_with_auth.layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default()))
}

async fn auth_test(auth_session: AuthSession) -> String {
    match auth_session.user {
        Some(user) => format!("Hello, {}!", user.full_name),
        None => "Hello, anonymous!".to_string(),
    }
}
