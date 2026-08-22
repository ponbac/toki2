use std::sync::Arc;

use axum::{handler::Handler, http::Method, routing::get, Router};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    login_required,
    tower_sessions::{CachingSessionStore, ExpiredDeletion, Expiry, SessionManagerLayer},
    AuthManagerLayer, AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sqlx::PgPool;
use time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_sessions_moka_store::MokaStore;
use tower_sessions_sqlx_store::PostgresStore;
use tracing::Instrument;

type SessionStore = CachingSessionStore<MokaStore, PostgresStore>;
const SESSION_COOKIE_NAME: &str = "toki.sid";

use crate::{
    adapters::outbound::{
        media::WebpAvatarProcessor,
        postgres::{PostgresApiTokenRepository, PostgresAvatarRepository},
    },
    app_state::AppState,
    auth::{self, AuthBackend},
    config::Settings,
    db::DbPool,
    domain::{
        ports::inbound::{ApiTokenAuthenticator, ApiTokenService, AvatarService},
        services::{ApiTokenServiceImpl, AvatarServiceImpl},
        RepoConfig,
    },
    factory::KleerServiceFactory,
    routes,
};

pub async fn create(
    connection_pool: PgPool,
    db_pool: DbPool,
    repo_configs: Vec<RepoConfig>,
    config: Settings,
) -> Router<()> {
    let base_app = Router::new()
        .route("/", get(|| async { "Hello, little World!" }))
        .nest("/pull-requests", routes::pull_requests::router())
        .nest("/differs", routes::differs::router())
        .nest("/repositories", routes::repositories::router())
        .nest("/notifications", routes::notifications::router())
        .nest("/time-tracking", routes::time_tracking::router())
        .nest("/users", routes::users::router())
        .nest("/work-items", routes::work_items::router());

    let api_tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(db_pool.clone()),
    )));
    let api_token_service: Arc<dyn ApiTokenService> = api_tokens.clone();
    let api_token_authenticator: Arc<dyn ApiTokenAuthenticator> = api_tokens;

    // If authentication is enabled, wrap the app with the auth middleware
    let app_with_auth = if config.application.disable_auth {
        base_app.merge(Router::new().route(
            "/time-tracking/timer",
            get(routes::time_tracking::get_timer_status),
        ))
    } else {
        let auth_layer =
            new_auth_layer(connection_pool.clone(), db_pool.clone(), config.clone()).await;
        authenticated_routes(base_app, api_token_authenticator).layer(auth_layer)
    };

    // Create the time tracking factory (composition root wiring)
    let timer_repo = Arc::new(crate::repositories::TimerRepositoryImpl::new(
        db_pool.clone(),
    ));
    let time_tracking_user_link_repo =
        Arc::new(crate::repositories::TimeTrackingUserLinkRepositoryImpl::new(db_pool.clone()));
    let time_tracking_factory = Arc::new(KleerServiceFactory::new(
        timer_repo,
        time_tracking_user_link_repo,
        config.kleer.clone(),
    ));
    let avatar_repository = Arc::new(PostgresAvatarRepository::new(db_pool.clone()));
    let avatar_processor = Arc::new(WebpAvatarProcessor);
    let avatar_service: Arc<dyn AvatarService> = Arc::new(AvatarServiceImpl::new(
        avatar_repository,
        avatar_processor,
        config.application.api_url.clone(),
    ));

    // Create app state
    let app_state = AppState::new(
        config.application.app_url.clone(),
        config.application.api_url.clone(),
        config.kleer.clone(),
        db_pool,
        repo_configs,
        time_tracking_factory,
        avatar_service,
        api_token_service,
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
        .layer(axum::middleware::from_fn_with_state(
            config.observability.clone(),
            crate::observability::capture_request_body_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::observability::log_http_response_middleware,
        ))
        .layer(crate::observability::http_trace_layer())
}

fn authenticated_routes(
    session_routes: Router<AppState>,
    api_token_authenticator: Arc<dyn ApiTokenAuthenticator>,
) -> Router<AppState> {
    compose_authenticated_routes(
        session_routes,
        routes::time_tracking::get_timer_status,
        auth::router(),
        api_token_authenticator,
    )
}

fn compose_authenticated_routes<S, H, T>(
    session_routes: Router<S>,
    timer_status_handler: H,
    public_auth_routes: Router<S>,
    api_token_authenticator: Arc<dyn ApiTokenAuthenticator>,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    H: Handler<T, S>,
    T: 'static,
{
    // Construct the only bearer-capable route here so callers cannot widen the
    // token's authority by passing a larger router or additional HTTP methods.
    let timer_status_routes = Router::new()
        .route("/time-tracking/timer", get(timer_status_handler))
        .route_layer(axum::middleware::from_fn(auth::require_authenticated))
        .layer(axum::middleware::from_fn_with_state(
            api_token_authenticator,
            auth::authenticate_bearer,
        ));

    session_routes
        .route_layer(login_required!(AuthBackend))
        .merge(timer_status_routes)
        .merge(public_auth_routes)
}

async fn new_auth_layer(
    connection_pool: PgPool,
    db_pool: DbPool,
    config: Settings,
) -> AuthManagerLayer<AuthBackend, SessionStore> {
    let client = BasicClient::new(ClientId::new(config.auth.client_id))
        .set_client_secret(ClientSecret::new(config.auth.client_secret))
        .set_auth_uri(
            AuthUrl::new(config.auth.auth_url).expect("Invalid authorization endpoint URL"),
        )
        .set_token_uri(TokenUrl::new(config.auth.token_url).expect("Invalid token endpoint URL"))
        .set_redirect_uri(
            RedirectUrl::new(config.auth.redirect_url).expect("Invalid redirect URL"),
        );

    // Use PostgresStore for DB-backed sessions that persist across restarts
    let db_store = PostgresStore::new(connection_pool.clone());
    db_store
        .migrate()
        .instrument(crate::observability::db_operation_span(
            &config.database,
            "MIGRATE",
            "tower_sessions_sqlx_store::PostgresStore::migrate()",
        ))
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

    let secure_cookies = config.application.api_url.starts_with("https://");
    let session_layer = SessionManagerLayer::new(session_store)
        .with_name(SESSION_COOKIE_NAME)
        .with_secure(secure_cookies)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    let backend = AuthBackend::new(db_pool, client);
    AuthManagerLayerBuilder::new(backend, session_layer).build()
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use oauth2::ClientSecret;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use crate::{
        config::DatabaseSettings,
        domain::{ApiTokenError, Role, UserPrincipal},
    };

    use super::*;

    struct StubAuthenticator;

    #[async_trait]
    impl ApiTokenAuthenticator for StubAuthenticator {
        async fn authenticate(
            &self,
            _presented: &str,
        ) -> Result<Option<UserPrincipal>, ApiTokenError> {
            Ok(Some(UserPrincipal {
                id: crate::domain::models::UserId::new(7),
                email: "ada@example.com".to_string(),
                roles: vec![Role::User],
            }))
        }
    }

    #[tokio::test]
    async fn api_tokens_are_scoped_to_get_timer_status_in_production_composition() {
        let base_routes = Router::new()
            .route(
                "/time-tracking/timer",
                axum::routing::post(|| async { StatusCode::OK })
                    .put(|| async { StatusCode::OK })
                    .delete(|| async { StatusCode::OK }),
            )
            .route("/users/me/api-tokens", get(|| async { StatusCode::OK }));
        let public_auth_routes = Router::new().route(
            "/me",
            get(|session: crate::auth::AuthSession| async move {
                if session.user.is_some() {
                    StatusCode::OK
                } else {
                    StatusCode::UNAUTHORIZED
                }
            }),
        );
        let app = compose_authenticated_routes(
            base_routes,
            |user: crate::auth::AuthUser| async move { user.id.to_string() },
            public_auth_routes,
            Arc::new(StubAuthenticator),
        )
        .layer(test_auth_layer());

        for (method, path, expected) in [
            (Method::GET, "/time-tracking/timer", StatusCode::OK),
            (
                Method::POST,
                "/time-tracking/timer",
                StatusCode::UNAUTHORIZED,
            ),
            (
                Method::PUT,
                "/time-tracking/timer",
                StatusCode::UNAUTHORIZED,
            ),
            (
                Method::DELETE,
                "/time-tracking/timer",
                StatusCode::UNAUTHORIZED,
            ),
            (Method::GET, "/me", StatusCode::UNAUTHORIZED),
            (
                Method::GET,
                "/users/me/api-tokens",
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header(axum::http::header::AUTHORIZATION, "Bearer toki_secret")
                        .body(Body::empty())
                        .expect("valid test request"),
                )
                .await
                .expect("router should respond");
            assert_eq!(response.status(), expected, "{path}");
        }
    }

    fn test_auth_layer() -> AuthManagerLayer<AuthBackend, MokaStore> {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:password@localhost/toki_test")
            .expect("valid lazy database URL");
        let database = DatabaseSettings {
            username: "postgres".to_string(),
            password: "password".to_string(),
            port: 5432,
            host: "localhost".to_string(),
            database_name: "toki_test".to_string(),
            require_ssl: false,
        };
        let backend = AuthBackend::new(
            crate::db::traced_pool(pool, &database),
            BasicClient::new(ClientId::new("test-client".to_string()))
                .set_client_secret(ClientSecret::new("test-secret".to_string()))
                .set_auth_uri(
                    AuthUrl::new("https://example.com/authorize".to_string())
                        .expect("valid auth URL"),
                )
                .set_token_uri(
                    TokenUrl::new("https://example.com/token".to_string())
                        .expect("valid token URL"),
                )
                .set_redirect_uri(
                    RedirectUrl::new("https://example.com/callback".to_string())
                        .expect("valid redirect URL"),
                ),
        );
        let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);
        AuthManagerLayerBuilder::new(backend, sessions).build()
    }
}
