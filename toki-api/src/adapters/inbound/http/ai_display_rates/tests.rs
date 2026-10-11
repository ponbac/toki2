use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use time::OffsetDateTime;
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::outbound::postgres::{
        ai_usage_fixtures::insert_user, PostgresAiExchangeRateRepository,
        PostgresApiTokenRepository,
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend},
    domain::{
        models::{
            AiBillingMonth, AiCurrency, AiExchangeRateValue, AiProvidedRate, AiUsageTimeZone,
        },
        ports::{
            inbound::{ApiTokenAuthenticator, ApiTokenService},
            outbound::ExchangeRateProvider,
        },
        services::{
            AiDisplayRateServiceImpl, AiExchangeRateSettings, AiExchangeRates, ApiTokenServiceImpl,
        },
        ExchangeRateError,
    },
};

/// A stand-in exchange-rate provider with made-up rates.
#[derive(Default)]
struct FakeRates {
    /// Final monthly averages by `(YYYY-MM, currency)`.
    monthly: HashMap<(String, String), &'static str>,
    /// The average of a month's days so far by currency, whatever the month.
    daily: HashMap<String, &'static str>,
    requests: Mutex<Vec<String>>,
}

impl FakeRates {
    fn monthly(mut self, month: &str, currency: &str, rate: &'static str) -> Self {
        self.monthly
            .insert((month.to_string(), currency.to_string()), rate);
        self
    }

    fn daily(mut self, currency: &str, rate: &'static str) -> Self {
        self.daily.insert(currency.to_string(), rate);
        self
    }

    fn answer(&self, request: String, rate: Option<&&'static str>, through: Date) -> Answer {
        self.requests.lock().unwrap().push(request);
        Ok(rate
            .and_then(|rate| AiExchangeRateValue::parse(rate))
            .map(|rate| AiProvidedRate {
                rate,
                observed_through: through,
            }))
    }
}

type Answer = Result<Option<AiProvidedRate>, ExchangeRateError>;

#[async_trait]
impl ExchangeRateProvider for FakeRates {
    fn name(&self) -> &'static str {
        "example"
    }

    fn may_have_published_after(&self, _: Date, _: OffsetDateTime) -> bool {
        true
    }

    async fn monthly_average(&self, currency: &AiCurrency, month: AiBillingMonth) -> Answer {
        let key = (month.to_string(), currency.as_str().to_string());
        self.answer(
            format!("monthly {} {month}", currency.as_str()),
            self.monthly.get(&key),
            month.last_day(),
        )
    }

    async fn daily_average(&self, currency: &AiCurrency, from: Date, through: Date) -> Answer {
        self.answer(
            format!("daily {} {from}", currency.as_str()),
            self.daily.get(currency.as_str()),
            through,
        )
    }
}

/// The route composed as in production, behind authentication.
fn app(pool: &PgPool, rates: Arc<FakeRates>) -> Router {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let service: Arc<dyn AiDisplayRateService> =
        Arc::new(AiDisplayRateServiceImpl::new(AiExchangeRates::new(
            Arc::new(PostgresAiExchangeRateRepository::new(db.clone())),
            rates,
            AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
            AiExchangeRateSettings::default(),
        )));
    let tokens: Arc<dyn ApiTokenAuthenticator> = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(db.clone()),
    )));
    let client = BasicClient::new(ClientId::new("test-client".to_string()))
        .set_client_secret(ClientSecret::new("test-secret".to_string()))
        .set_auth_uri(AuthUrl::new("https://example.com/authorize".to_string()).unwrap())
        .set_token_uri(TokenUrl::new("https://example.com/token".to_string()).unwrap())
        .set_redirect_uri(RedirectUrl::new("https://example.com/callback".to_string()).unwrap());
    let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);

    Router::new()
        .nest("/ai-usage", router())
        .route_layer(middleware::from_fn(require_authenticated))
        .layer(middleware::from_fn_with_state(tokens, authenticate_bearer))
        .with_state(service)
        .layer(AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build())
}

/// A bearer token of a new user.
async fn token(pool: &PgPool) -> String {
    let user = insert_user(pool, "dev@example.com", "User").await;
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let issued = ApiTokenServiceImpl::new(Arc::new(PostgresApiTokenRepository::new(db)))
        .create(&user, "token-ledger")
        .await
        .unwrap();
    format!("Bearer {}", issued.secret.as_str())
}

async fn get(app: &Router, token: Option<&str>, uri: &str) -> (StatusCode, Value) {
    let mut request = Request::get(uri);
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, token);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[sqlx::test]
async fn a_range_shows_at_the_rates_its_last_month_bills_at(pool: PgPool) {
    let rates = Arc::new(
        FakeRates::default()
            .monthly("2026-08", "USD", "10.5")
            .monthly("2026-08", "EUR", "11.2"),
    );
    let app = app(&pool, rates.clone());
    let token = token(&pool).await;

    let (status, body) = get(&app, Some(&token), "/ai-usage/display-rates?to=2026-08-20").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "month": "2026-08",
            "rates": [
                { "currency": "SEK", "perUsd": "10.50", "provisional": false, "overridden": false },
                { "currency": "EUR", "perUsd": "0.9375", "provisional": false, "overridden": false },
            ],
            "pending": false,
        })
    );

    // The final rates are stored: showing the month again fetches nothing.
    let fetched = rates.requests.lock().unwrap().len();
    get(&app, Some(&token), "/ai-usage/display-rates?to=2026-08-01").await;
    assert_eq!(rates.requests.lock().unwrap().len(), fetched);
}

#[sqlx::test]
async fn the_month_before_stands_in_for_a_month_without_a_usd_rate(pool: PgPool) {
    let rates = Arc::new(
        FakeRates::default()
            .monthly("2026-07", "USD", "9.8")
            .monthly("2026-08", "EUR", "11.2"),
    );
    let app = app(&pool, rates);
    let token = token(&pool).await;

    let (status, body) = get(&app, Some(&token), "/ai-usage/display-rates?to=2026-08-20").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "month": "2026-07",
            "rates": [
                { "currency": "SEK", "perUsd": "9.80", "provisional": false, "overridden": false },
            ],
            "pending": false,
        })
    );

    // With no rate in either month, nothing converts.
    let (status, body) = get(&app, Some(&token), "/ai-usage/display-rates?to=2026-06-30").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "month": "2026-06", "rates": [], "pending": false })
    );
}

#[sqlx::test]
async fn usage_to_come_shows_at_the_current_months_rate_so_far(pool: PgPool) {
    let rates = Arc::new(FakeRates::default().daily("USD", "9.42"));
    let app = app(&pool, rates);
    let token = token(&pool).await;

    let (status, current) = get(&app, Some(&token), "/ai-usage/display-rates").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        current["rates"],
        json!([
            { "currency": "SEK", "perUsd": "9.42", "provisional": true, "overridden": false },
        ])
    );

    let (status, later) = get(&app, Some(&token), "/ai-usage/display-rates?to=9999-12-31").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(later, current);
}

#[sqlx::test]
async fn rates_need_a_signed_in_user_and_a_real_day(pool: PgPool) {
    let app = app(&pool, Arc::new(FakeRates::default()));
    let token = token(&pool).await;

    let (status, _) = get(&app, None, "/ai-usage/display-rates").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    for uri in [
        "/ai-usage/display-rates?to=20-08-2026",
        "/ai-usage/display-rates?to=0000-01-01",
    ] {
        let (status, body) = get(&app, Some(&token), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(body["error"].is_string(), "{uri}: {body}");
    }
}
