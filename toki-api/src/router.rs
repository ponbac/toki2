use axum::{http::Method, routing::get, Router};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    login_required,
    tower_sessions::{CachingSessionStore, ExpiredDeletion, Expiry, SessionManagerLayer},
    AuthManagerLayer, AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sqlx::PgPool;
use time::Duration;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::{DefaultMakeSpan, TraceLayer},
};
use tower_sessions_moka_store::MokaStore;
use tower_sessions_sqlx_store::PostgresStore;

type SessionStore = CachingSessionStore<MokaStore, PostgresStore>;

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
        let auth_layer = new_auth_layer(connection_pool.clone(), config.clone()).await;
        base_app
            .route_layer(login_required!(AuthBackend))
            .merge(auth::router())
            .layer(auth_layer)
    };

    // Create app state
    let app_state = AppState::new(
        config.application.app_url.clone(),
        config.application.api_url.clone(),
        config.application.cookie_domain.clone(),
        connection_pool.clone(),
        repo_configs,
    )
    .await;

    // Start all the differ threads (if in production)
    #[cfg(not(debug_assertions))]
    app_state.start_all_differs().await;

    // Finally, wrap the app with tracing layer, state and CORS
    let app_url = config.application.app_url.clone();
    let allowed_suffix = config.application.cors_allowed_origin_suffix.clone();
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(["content-type".parse().unwrap()])
        .allow_credentials(true)
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            let origin_str = origin.to_str().unwrap_or_default();
            if origin_str == app_url {
                return true;
            }
            if let Some(ref suffix) = allowed_suffix {
                return origin_str.starts_with("https://") && origin_str.ends_with(suffix.as_str());
            }
            false
        }));
    app_with_auth
        .with_state(app_state)
        .layer(cors)
        .layer(TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default()))
}

async fn new_auth_layer(
    connection_pool: PgPool,
    config: Settings,
) -> AuthManagerLayer<AuthBackend, SessionStore> {
    let client = BasicClient::new(
        ClientId::new(config.auth.client_id),
        Some(ClientSecret::new(config.auth.client_secret)),
        AuthUrl::new(config.auth.auth_url).expect("Invalid authorization endpoint URL"),
        Some(TokenUrl::new(config.auth.token_url).expect("Invalid token endpoint URL")),
    )
    .set_redirect_uri(RedirectUrl::new(config.auth.redirect_url).expect("Invalid redirect URL"));

    // Use PostgresStore for DB-backed sessions that persist across restarts
    let db_store = PostgresStore::new(connection_pool.clone());
    db_store
        .migrate()
        .await
        .expect("Failed to run session store migration");

    // Spawn background task to clean up expired sessions from DB
    let deletion_task = tokio::task::spawn(
        db_store
            .clone()
            .continuously_delete_expired(tokio::time::Duration::from_secs(60)),
    );
    // Detach the task so it runs independently
    drop(deletion_task);

    // Wrap with in-memory Moka cache to reduce DB reads for hot sessions
    let cache_store = MokaStore::new(Some(2_000));
    let session_store = CachingSessionStore::new(cache_store, db_store);

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false) // todo: explore production values
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    let backend = AuthBackend::new(connection_pool, client);
    AuthManagerLayerBuilder::new(backend, session_layer).build()
}
