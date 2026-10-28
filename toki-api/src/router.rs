use std::sync::Arc;

use axum::{http::Method, routing::get, Router};
use axum_extra::extract::cookie::SameSite;
use axum_login::{
    tower_sessions::{CachingSessionStore, ExpiredDeletion, Expiry, SessionManagerLayer},
    AuthManagerLayer, AuthManagerLayerBuilder,
};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use sqlx::{ConnectOptions, PgPool};
use time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_sessions_moka_store::MokaStore;
use tower_sessions_sqlx_store::PostgresStore;
use tracing::Instrument;
use url::Url;

type SessionStore = CachingSessionStore<MokaStore, PostgresStore>;
const SESSION_COOKIE_NAME: &str = "toki.sid";

use crate::{
    adapters::{
        inbound::{
            http::{self, agent_openapi, openapi_spec_router},
            mcp,
        },
        outbound::{
            cimd::CimdFetcher,
            github::{self, GithubAccountDirectory, GithubApp, GithubMeteredUsageSource},
            media::WebpAvatarProcessor,
            postgres::{
                PostgresAiAdoptionRepository, PostgresAiBillingRepository,
                PostgresAiExchangeRateRepository, PostgresAiMeteredUsageRepository,
                PostgresAiMonthCloseRepository, PostgresAiProjectMappingRepository,
                PostgresAiProviderAccountRepository, PostgresAiRepositoryMappingRepository,
                PostgresAiSubscriptionRepository, PostgresAiUsageReportRepository,
                PostgresAiUsageRepository, PostgresApiTokenRepository, PostgresAvatarRepository,
                PostgresOAuthRepository,
            },
            riksbank::RiksbankExchangeRateAdapter,
        },
    },
    app_state::AppState,
    auth::{self, AuthBackend},
    config::Settings,
    db::DbPool,
    domain::{
        models::{
            AiAdoptionCredits, AiBillingMonth, AiImportFreshness, AiMeteredCutover, AiProvider,
            AiUsageTimeZone,
        },
        ports::{
            inbound::{
                AiAdoptionService, AiBillingService, AiDisplayRateService, AiMeteredUsageService,
                AiProjectMappingService, AiProviderAccountService, AiRepositoryMappingService,
                AiSubscriptionService, AiUsageReportService, AiUsageService, ApiTokenAuthenticator,
                ApiTokenService, AvatarService, OAuthAccessTokenAuthenticator, OAuthService,
            },
            outbound::TimeTrackingProjectCatalog,
        },
        services::{
            AiAccountDirectoryState, AiAdoptionServiceImpl, AiBillingServiceImpl,
            AiDisplayRateServiceImpl, AiExchangeRateSettings, AiExchangeRates,
            AiMeteredImportSettings, AiMeteredMonths, AiMeteredReadSettings, AiMeteredSourceState,
            AiMeteredUsageServiceImpl, AiProjectMappingServiceImpl, AiProviderAccountServiceImpl,
            AiRepositoryMappingServiceImpl, AiRepositoryMappingSettings, AiSubscriptionServiceImpl,
            AiUsageReportServiceImpl, AiUsageServiceImpl, ApiTokenServiceImpl, AvatarServiceImpl,
            OAuthServiceImpl,
        },
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
    // The OAuth authorization server MCP clients sign in through.
    let api_url = Url::parse(&config.application.api_url).expect("Invalid API URL");
    let oauth = Arc::new(OAuthServiceImpl::new(
        Arc::new(PostgresOAuthRepository::new(db_pool.clone())),
        Arc::new(CimdFetcher::new().expect("Failed to build the client metadata fetcher")),
        mcp::resource_url(&api_url),
    ));
    let oauth_service: Arc<dyn OAuthService> = oauth.clone();

    let base_app = Router::new()
        .route("/", get(|| async { "Hello, little World!" }))
        .nest("/pull-requests", routes::pull_requests::router())
        .nest("/differs", routes::differs::router())
        .nest("/repositories", routes::repositories::router())
        .nest("/notifications", routes::notifications::router())
        .nest("/time-tracking", routes::time_tracking::router())
        .nest(
            "/users",
            routes::users::router().nest(
                "/me/oauth-grants",
                http::oauth::grants_router(oauth_service.clone()),
            ),
        )
        .nest("/work-items", routes::work_items::router())
        .nest(
            "/ai-usage",
            http::ai_usage::router()
                .merge(http::ai_usage_reports::router())
                .merge(http::ai_display_rates::router())
                .merge(http::ai_subscriptions::router()),
        )
        .nest(
            "/ai-usage/project-mappings",
            http::ai_project_mappings::router(),
        )
        .nest(
            "/ai-usage/admin",
            http::ai_billing::router()
                .merge(http::ai_adoption::router())
                .merge(http::ai_provider_accounts::router())
                .merge(http::ai_metered_usage::router())
                .merge(http::ai_repository_mappings::router()),
        );

    let api_tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(db_pool.clone()),
    )));
    let api_token_service: Arc<dyn ApiTokenService> = api_tokens.clone();
    let api_token_authenticator: Arc<dyn ApiTokenAuthenticator> = api_tokens;
    let mcp_api_token_authenticator = api_token_authenticator.clone();

    let automation_openapi = agent_openapi();

    // If authentication is enabled, wrap the app with the auth middleware
    let app_with_auth = if config.application.disable_auth {
        base_app
    } else {
        let auth_layer =
            new_auth_layer(connection_pool.clone(), db_pool.clone(), config.clone()).await;
        let oauth_server = http::oauth::server_router(
            oauth_service.clone(),
            http::oauth::OAuthServerConfig {
                issuer: oauth_issuer(&api_url),
                api_origin: api_url.origin().ascii_serialization(),
            },
        );
        spawn_oauth_cleanup(oauth_service.clone());
        authenticated_routes(base_app, oauth_server, api_token_authenticator).layer(auth_layer)
    }
    .merge(openapi_spec_router(automation_openapi));

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

    let ai_usage_repository = Arc::new(PostgresAiUsageRepository::new(db_pool.clone()));
    let ai_usage_time_zone = AiUsageTimeZone::parse(&config.ai_usage.time_zone)
        .expect("ai_usage.time_zone must be an IANA time zone name");
    let time_zone_known = ai_usage_repository
        .knows_time_zone(&ai_usage_time_zone)
        .await
        .expect("Failed to check ai_usage.time_zone");
    assert!(
        time_zone_known,
        "ai_usage.time_zone {:?} is not a time zone the database knows",
        ai_usage_time_zone.as_str()
    );
    // Mappings accept any active project in the company, not only the projects
    // the caller can book time on: mappings are team-wide and admins map keys
    // for everyone. Time tracking may be unconfigured, as elsewhere; then no
    // mapping can be made or resolve, and this is reported once, here.
    let project_catalog = time_tracking_factory
        .project_catalog()
        .inspect_err(|error| {
            tracing::error!(
                "AI usage project mappings are unavailable: time tracking is not configured: {error}"
            );
        })
        .ok()
        .map(Arc::new);
    let mapping_company = project_catalog
        .as_ref()
        .map(|catalog| catalog.company().clone());
    let ai_project_mapping_service: Arc<dyn AiProjectMappingService> =
        Arc::new(AiProjectMappingServiceImpl::new(
            Arc::new(PostgresAiProjectMappingRepository::new(db_pool.clone())),
            project_catalog.clone(),
            ai_usage_time_zone.clone(),
        ));
    let ai_usage_service: Arc<dyn AiUsageService> = Arc::new(AiUsageServiceImpl::new(
        ai_usage_repository,
        config.ai_usage.reprice_floor,
        ai_usage_time_zone.clone(),
    ));
    let ai_subscription_repository =
        Arc::new(PostgresAiSubscriptionRepository::new(db_pool.clone()));
    let exchange_rate_provider = Arc::new(
        RiksbankExchangeRateAdapter::new(
            &config.ai_usage.exchange_rates.base_url,
            config.ai_usage.exchange_rates.subscription_key.clone(),
        )
        .expect("Failed to build the exchange rate client"),
    );
    // Billing and display rates share one fetcher: its stored rates, single
    // flights and provider backoff.
    let ai_exchange_rates = AiExchangeRates::new(
        Arc::new(PostgresAiExchangeRateRepository::new(db_pool.clone())),
        exchange_rate_provider,
        ai_usage_time_zone.clone(),
        AiExchangeRateSettings::default(),
    );
    let ai_display_rate_service: Arc<dyn AiDisplayRateService> =
        Arc::new(AiDisplayRateServiceImpl::new(ai_exchange_rates.clone()));
    // GitHub bills Copilot per seat and credit: Copilot is the payer whose
    // accounts and usage these are. The bill and the reconciliation tab read
    // its months through one loader.
    let ai_metered_usage_repository =
        Arc::new(PostgresAiMeteredUsageRepository::new(db_pool.clone()));
    let ai_provider_account_repository =
        Arc::new(PostgresAiProviderAccountRepository::new(db_pool.clone()));
    let metered_cutover = metered_cutover(config.ai_usage.metered_copilot_from.as_deref())
        .unwrap_or_else(|message| panic!("{message}"))
        .map(|from| AiMeteredCutover {
            payer: AiProvider::Copilot,
            from,
        });
    if let Some(cutover) = metered_cutover {
        let backfill_from = config.ai_usage.metered_import.backfill_from;
        if cutover.from.first_day() < backfill_from {
            tracing::warn!(
                cutover = %cutover.from,
                %backfill_from,
                "Copilot usage before the import's backfill shows as not imported"
            );
        }
    }
    // Copilot's code review credits bill per repository, to the project each
    // repository is mapped to; suggestions match GitHub's project keys.
    let ai_repository_mapping_service: Arc<dyn AiRepositoryMappingService> =
        Arc::new(AiRepositoryMappingServiceImpl::new(
            Arc::new(PostgresAiRepositoryMappingRepository::new(db_pool.clone())),
            project_catalog,
            AiProvider::Copilot,
            AiRepositoryMappingSettings {
                key_host: github::REPOSITORY_KEY_HOST.to_string(),
                billed_from: metered_cutover.map(|cutover| cutover.from),
            },
        ));
    // Adoption measures Copilot by GitHub's own credits wherever they are
    // imported: from the import's first day, not the bill's cutover month.
    let ai_adoption_service: Arc<dyn AiAdoptionService> = Arc::new(AiAdoptionServiceImpl::new(
        Arc::new(PostgresAiAdoptionRepository::new(db_pool.clone())),
        ai_usage_time_zone.clone(),
        metered_cutover.map(|cutover| AiAdoptionCredits {
            payer: cutover.payer,
            from: config.ai_usage.metered_import.backfill_from,
        }),
    ));
    let ai_billing_service: Arc<dyn AiBillingService> = Arc::new(AiBillingServiceImpl::new(
        Arc::new(PostgresAiBillingRepository::new(db_pool.clone())),
        ai_subscription_repository.clone(),
        ai_exchange_rates,
        ai_usage_time_zone.clone(),
        mapping_company.clone(),
        AiMeteredMonths::new(
            ai_metered_usage_repository.clone(),
            ai_provider_account_repository.clone(),
            AiProvider::Copilot,
            AiMeteredReadSettings {
                backfill_from: config.ai_usage.metered_import.backfill_from,
                reprice_floor: config.ai_usage.reprice_floor,
                time_zone: ai_usage_time_zone.clone(),
                mapping_company: mapping_company.clone(),
            },
        ),
        metered_cutover,
        // Closed months are served from their snapshots; later changes post as
        // adjustments on the next open month.
        Arc::new(PostgresAiMonthCloseRepository::new(db_pool.clone())),
        config.ai_usage.reprice_floor,
    ));
    // Personal usage reads: always the caller's own usage, admins included.
    let ai_usage_report_service: Arc<dyn AiUsageReportService> =
        Arc::new(AiUsageReportServiceImpl::new(
            Arc::new(PostgresAiUsageReportRepository::new(db_pool.clone())),
            ai_usage_time_zone.clone(),
            mapping_company.clone(),
        ));
    let ai_subscription_service: Arc<dyn AiSubscriptionService> = Arc::new(
        AiSubscriptionServiceImpl::new(ai_subscription_repository, ai_usage_time_zone.clone()),
    );
    // A misconfigured GitHub App never stops startup: the admin pages report
    // it, as time tracking reports missing Kleer credentials. Every GitHub
    // adapter shares one app: one client, token cache and pacer.
    let github: Result<Option<GithubApp>, String> = config
        .github
        .app_credentials()
        .and_then(|credentials| credentials.map(GithubApp::new).transpose());
    match &github {
        Ok(None) => tracing::info!(
            "GitHub is off (seats and usage import): TOKI_GITHUB__APP_PRIVATE_KEY is not set"
        ),
        Err(message) => tracing::error!(%message, "GitHub is misconfigured"),
        Ok(Some(_)) => {}
    }
    let account_directory = match &github {
        Ok(Some(app)) => {
            AiAccountDirectoryState::Ready(Arc::new(GithubAccountDirectory::new(app.clone())))
        }
        Ok(None) => AiAccountDirectoryState::Disabled,
        Err(message) => AiAccountDirectoryState::Misconfigured(message.clone()),
    };
    let usage_source = match &github {
        Ok(Some(app)) => {
            AiMeteredSourceState::Ready(Arc::new(GithubMeteredUsageSource::new(app.clone())))
        }
        Ok(None) => AiMeteredSourceState::Disabled,
        Err(message) => AiMeteredSourceState::Misconfigured(message.clone()),
    };
    #[cfg(not(debug_assertions))]
    let account_directory_ready = matches!(account_directory, AiAccountDirectoryState::Ready(_));
    #[cfg(not(debug_assertions))]
    let usage_source_ready = matches!(usage_source, AiMeteredSourceState::Ready(_));
    let ai_provider_account_service: Arc<dyn AiProviderAccountService> =
        Arc::new(AiProviderAccountServiceImpl::new(
            ai_provider_account_repository.clone(),
            account_directory,
            AiProvider::Copilot,
            config.github.organization.clone().unwrap_or_default(),
        ));
    let ai_metered_usage_service: Arc<dyn AiMeteredUsageService> =
        Arc::new(AiMeteredUsageServiceImpl::new(
            ai_metered_usage_repository,
            ai_provider_account_repository,
            usage_source,
            AiProvider::Copilot,
            config.github.organization.clone().unwrap_or_default(),
            AiMeteredImportSettings {
                backfill_from: config.ai_usage.metered_import.backfill_from,
                reprice_floor: config.ai_usage.reprice_floor,
                time_zone: ai_usage_time_zone,
                unattended: cfg!(not(debug_assertions)),
            },
            mapping_company,
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
        ai_usage_service,
        ai_usage_report_service,
        ai_project_mapping_service,
        ai_subscription_service,
        ai_adoption_service,
        ai_billing_service,
        ai_display_rate_service,
        ai_provider_account_service.clone(),
        ai_metered_usage_service.clone(),
        ai_repository_mapping_service,
    )
    .await;

    // Without sign-in there is no authorization server, so `/mcp` then takes
    // API tokens only.
    let mcp_access_tokens = (!config.application.disable_auth)
        .then(|| oauth.clone() as Arc<dyn OAuthAccessTokenAuthenticator>);
    let mcp_auth = mcp_auth(&app_state, mcp_api_token_authenticator, mcp_access_tokens);
    let app_with_auth = app_with_auth.merge(mcp::router(app_state.clone(), mcp_auth));

    // Start all the differ threads (if in production)
    #[cfg(not(debug_assertions))]
    app_state.start_all_differs().await;

    // Refresh provider accounts daily (if in production). Local runs, whose
    // default database is a production copy, never write seats unattended;
    // the admin Refresh works in every build.
    #[cfg(not(debug_assertions))]
    if account_directory_ready {
        spawn_provider_account_refresh(ai_provider_account_service.clone());
    }

    // Import metered usage hourly and nightly (if in production), for the
    // same reason; the admin Import now works in every build.
    #[cfg(not(debug_assertions))]
    if usage_source_ready {
        spawn_metered_usage_import(ai_metered_usage_service.clone());
    }

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

/// The metered cutover month from `ai_usage.metered_copilot_from`: `None`
/// when it is unset or blank (the rollback), an error naming the setting when
/// it is not `YYYY-MM`.
fn metered_cutover(raw: Option<&str>) -> Result<Option<AiBillingMonth>, String> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    AiBillingMonth::parse(raw)
        .map(Some)
        .ok_or_else(|| format!("ai_usage.metered_copilot_from must be YYYY-MM, got {raw:?}"))
}

/// `/mcp` accepts personal API tokens, and, given `access_tokens`, access
/// tokens from Toki's own authorization server.
fn mcp_auth(
    app_state: &AppState,
    api_tokens: Arc<dyn ApiTokenAuthenticator>,
    access_tokens: Option<Arc<dyn OAuthAccessTokenAuthenticator>>,
) -> mcp::McpAuth {
    let oauth = access_tokens.map(|access_tokens| mcp::McpOAuth {
        access_tokens,
        metadata: mcp::protected_resource_metadata(
            &app_state.api_url,
            oauth_issuer(&app_state.api_url),
        ),
        metadata_url: mcp::metadata_url(&app_state.api_url),
    });
    mcp::McpAuth { api_tokens, oauth }
}

/// The authorization server's issuer identifier: the API URL without a
/// trailing slash.
fn oauth_issuer(api_url: &Url) -> String {
    api_url.as_str().trim_end_matches('/').to_string()
}

/// Deletes expired authorization server records every hour.
fn spawn_oauth_cleanup(oauth: Arc<dyn OAuthService>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            if let Err(error) = oauth.remove_expired().await {
                tracing::error!(error = %error, "failed to delete expired OAuth records");
            }
        }
    });
}

/// Reads seats and profiles once a UTC day. Each hourly tick refreshes only
/// when due, so restarts add no calls and failures retry on the next tick.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn spawn_provider_account_refresh(service: Arc<dyn AiProviderAccountService>) {
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
        let mut ticks = tokio::time::interval_at(start, std::time::Duration::from_secs(60 * 60));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            match service.refresh_if_due().await {
                Ok(Some(refresh)) => tracing::info!(
                    seats = refresh.seats_listed,
                    ended = refresh.seats_ended,
                    "refreshed provider accounts on schedule"
                ),
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "provider account refresh failed"),
            }
        }
    });
}

/// Imports metered usage when due: hourly change detection, nightly full
/// refreshes. Each tick runs only what `scheduled_plan` says is due, so
/// restarts add no calls. A stale import is logged as a warning, the
/// freshness alert; nothing else is notified.
#[cfg_attr(debug_assertions, allow(dead_code))]
fn spawn_metered_usage_import(service: Arc<dyn AiMeteredUsageService>) {
    tokio::spawn(async move {
        let start = tokio::time::Instant::now() + std::time::Duration::from_secs(180);
        let mut ticks = tokio::time::interval_at(start, std::time::Duration::from_secs(60 * 60));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            match service.import_if_due().await {
                Ok(Some(run)) => tracing::info!(
                    status = run.status.as_str(),
                    requests = run.requests,
                    "imported metered usage on schedule"
                ),
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "metered usage import failed"),
            }
            match service.freshness().await {
                Ok(AiImportFreshness::Stale { reason }) => {
                    tracing::warn!(%reason, "the GitHub usage import is stale")
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "could not judge metered usage freshness"),
            }
        }
    });
}

fn authenticated_routes(
    protected_routes: Router<AppState>,
    oauth_server: Router<AppState>,
    api_token_authenticator: Arc<dyn ApiTokenAuthenticator>,
) -> Router<AppState> {
    compose_authenticated_routes(
        protected_routes,
        auth::router().merge(oauth_server),
        api_token_authenticator,
    )
}

fn compose_authenticated_routes<S>(
    protected_routes: Router<S>,
    public_routes: Router<S>,
    api_token_authenticator: Arc<dyn ApiTokenAuthenticator>,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let protected_routes = protected_routes
        .route_layer(axum::middleware::from_fn(auth::require_authenticated))
        .layer(axum::middleware::from_fn_with_state(
            api_token_authenticator,
            auth::authenticate_bearer,
        ));

    protected_routes.merge(public_routes)
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

    // The published session adapter uses SQLx 0.8. Give it a private pool with
    // the same connection settings while application repositories use SQLx 0.9.
    let connection_options = connection_pool.connect_options();
    let mut session_url = connection_options.to_url_lossy();
    // SQLx's URL conversion omits these two optional connection settings.
    if let Some(application_name) = connection_options.get_application_name() {
        session_url
            .query_pairs_mut()
            .append_pair("application_name", application_name);
    }
    if let Some(options) = connection_options.get_options() {
        session_url
            .query_pairs_mut()
            .append_pair("options", options);
    }
    let session_pool = sqlx_session::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(session_url.as_str())
        .expect("Invalid session database connection settings");
    let db_store = PostgresStore::new(session_pool);
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
    async fn api_tokens_authenticate_all_protected_routes() {
        let base_routes = Router::new()
            .route(
                "/time-tracking/timer",
                get(|| async { StatusCode::OK })
                    .post(|| async { StatusCode::OK })
                    .put(|| async { StatusCode::OK })
                    .delete(|| async { StatusCode::OK }),
            )
            .route("/users/me/api-tokens", get(|| async { StatusCode::OK }));
        let automation_openapi = agent_openapi();
        let public_routes = Router::new()
            .route("/public", get(|| async { StatusCode::OK }))
            .route(
                "/me",
                get(|session: crate::auth::AuthSession| async move {
                    if session.user.is_some() {
                        StatusCode::OK
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                }),
            );
        let app =
            compose_authenticated_routes(base_routes, public_routes, Arc::new(StubAuthenticator))
                .merge(openapi_spec_router(automation_openapi))
                .layer(test_auth_layer());

        for (method, path, expected) in [
            (Method::GET, "/time-tracking/timer", StatusCode::OK),
            (Method::POST, "/time-tracking/timer", StatusCode::OK),
            (Method::PUT, "/time-tracking/timer", StatusCode::OK),
            (Method::DELETE, "/time-tracking/timer", StatusCode::OK),
            (Method::GET, "/me", StatusCode::UNAUTHORIZED),
            (Method::GET, "/users/me/api-tokens", StatusCode::OK),
            (Method::GET, "/public", StatusCode::OK),
            (Method::GET, "/openapi.json", StatusCode::OK),
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

        let anonymous_protected = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/time-tracking/timer")
                    .body(Body::empty())
                    .expect("valid test request"),
            )
            .await
            .expect("router should respond");
        assert_eq!(anonymous_protected.status(), StatusCode::UNAUTHORIZED);

        let anonymous_public = app
            .oneshot(
                Request::builder()
                    .uri("/public")
                    .body(Body::empty())
                    .expect("valid test request"),
            )
            .await
            .expect("router should respond");
        assert_eq!(anonymous_public.status(), StatusCode::OK);
    }

    #[test]
    fn the_metered_cutover_is_a_month_or_nothing() {
        assert_eq!(metered_cutover(None), Ok(None));
        assert_eq!(metered_cutover(Some("")), Ok(None));
        assert_eq!(metered_cutover(Some("  ")), Ok(None));
        assert_eq!(
            metered_cutover(Some(" 2026-10 ")),
            Ok(AiBillingMonth::parse("2026-10"))
        );
        for invalid in ["2026-13", "2026-1", "October"] {
            let error = metered_cutover(Some(invalid)).unwrap_err();
            assert!(error.contains("ai_usage.metered_copilot_from"), "{error}");
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
