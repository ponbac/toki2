use axum::{http::Method, routing::get, Router};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    login_required,
    tower_sessions::{Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayer, AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sqlx::PgPool;
use time::Duration;
use tower_http::{
    cors::CorsLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};

use crate::{
    app_state::AppState,
    auth::{self, AuthBackend},
    config::Settings,
    domain::RepoConfig,
    routes,
};

pub async fn create(
    connection_pool: PgPool,
    repo_configs: Vec<RepoConfig>,
    config: Settings,
) -> Router<()> {
    let base_app = Router::new()
        .route("/", get(|| async { "Hello, little World!" }))
        .nest("/pull-requests", routes::pull_requests::router())
        .nest("/differs", routes::differs::router())
        .nest("/repositories", routes::repositories::router())
        .nest("/notifications", routes::notifications::router())
        .nest("/milltime", routes::milltime::router());

    // If authentication is enabled, wrap the app with the auth middleware
    let app_with_auth = if config.application.disable_auth {
        base_app
    } else {
        let auth_layer = new_auth_layer(connection_pool.clone(), config.clone());
        base_app
            .route_layer(login_required!(AuthBackend))
            .merge(auth::router())
            .layer(auth_layer)
    };

    // Finally, wrap the app with tracing layer, state and CORS
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(["content-type".parse().unwrap()])
        .allow_credentials(true)
        .allow_origin([config.application.app_url.parse().unwrap()]);
    app_with_auth
        .with_state(
            AppState::new(
                config.application.app_url,
                config.application.api_url,
                connection_pool.clone(),
                repo_configs,
            )
            .await,
        )
        .layer(cors)
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default()))
}

fn new_auth_layer(
    connection_pool: PgPool,
    config: Settings,
) -> AuthManagerLayer<AuthBackend, MemoryStore> {
    let client = BasicClient::new(
        ClientId::new(config.auth.client_id),
        Some(ClientSecret::new(config.auth.client_secret)),
        AuthUrl::new(config.auth.auth_url).expect("Invalid authorization endpoint URL"),
        Some(TokenUrl::new(config.auth.token_url).expect("Invalid token endpoint URL")),
    )
    .set_redirect_uri(RedirectUrl::new(config.auth.redirect_url).expect("Invalid redirect URL"));

    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // todo: explore production values
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    let backend = AuthBackend::new(connection_pool, client);
    AuthManagerLayerBuilder::new(backend, session_layer).build()
}
