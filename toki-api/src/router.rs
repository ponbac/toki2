use axum::{http::Method, routing::get, Router};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    login_required,
    tower_sessions::{Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayer, AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc};
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
    services::AvatarCacheService,
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
        .nest("/milltime", routes::milltime::router())
        .nest("/avatars", routes::avatars::router());

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

    // Create avatar cache service
    let avatar_cache = Arc::new(AvatarCacheService::new(
        PathBuf::from(&config.avatar_cache.cache_dir),
        Duration::hours(config.avatar_cache.ttl_hours as i64),
        config.avatar_cache.max_cache_size_mb * 1024 * 1024,
        config.avatar_cache.max_image_size_mb * 1024 * 1024,
        config.application.api_url.clone(),
    ));

    // Initialize avatar cache
    if let Err(e) = avatar_cache.initialize().await {
        tracing::error!("Failed to initialize avatar cache: {}", e);
    }

    // Start avatar cache cleanup task
    avatar_cache.start_cleanup_task();

    // Create app state
    let app_state = AppState::new(
        config.application.app_url.clone(),
        config.application.api_url.clone(),
        config.application.cookie_domain.clone(),
        connection_pool.clone(),
        repo_configs,
        avatar_cache,
    )
    .await;

    // Start all the differ threads (if in production)
    #[cfg(not(debug_assertions))]
    app_state.start_all_differs().await;

    // Finally, wrap the app with tracing layer, state and CORS
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(["content-type".parse().unwrap()])
        .allow_credentials(true)
        .allow_origin([config.application.app_url.parse().unwrap()]);
    app_with_auth
        .with_state(app_state)
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
