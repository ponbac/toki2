use axum::{
    body::{to_bytes, Body},
    extract::Path,
    http::{
        header::{AUTHORIZATION, COOKIE, SET_COOKIE},
        Request, StatusCode,
    },
    middleware,
    routing::post,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use time::macros::datetime;
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::outbound::postgres::{
        ai_usage_fixtures::{bucket, insert_user, record, APP, MACHINE_A, MACHINE_B},
        PostgresAiUsageReportRepository, PostgresApiTokenRepository,
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{AiUsageTimeZone, TimeTrackingCompany, UserId},
        ports::inbound::{ApiTokenAuthenticator, ApiTokenService},
        services::{AiUsageReportServiceImpl, ApiTokenServiceImpl},
    },
};

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The routes composed as in production: bearer tokens and browser sessions
/// both authenticate.
fn app(pool: &PgPool) -> Router {
    app_with(
        pool,
        Some(TimeTrackingCompany {
            provider: "kleer".to_string(),
            company_id: "company-1".to_string(),
        }),
    )
}

/// The routes with `company` as the configured time-tracking company, or
/// without time tracking for `None`.
fn app_with(pool: &PgPool, company: Option<TimeTrackingCompany>) -> Router {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let service: Arc<dyn AiUsageReportService> = Arc::new(AiUsageReportServiceImpl::new(
        Arc::new(PostgresAiUsageReportRepository::new(db.clone())),
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
        company,
    ));
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
        .route("/test-sign-in/{user_id}", post(sign_in))
        .with_state(service)
        .layer(AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build())
}

/// Request credentials as a header: a bearer API token or a session cookie.
struct Credential(&'static str, String);

async fn token(pool: &PgPool, user: UserId) -> Credential {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let issued = ApiTokenServiceImpl::new(Arc::new(PostgresApiTokenRepository::new(db)))
        .create(&user, "token-ledger")
        .await
        .unwrap();
    Credential(
        AUTHORIZATION.as_str(),
        format!("Bearer {}", issued.secret.as_str()),
    )
}

async fn session(app: &Router, user: UserId) -> Credential {
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/test-sign-in/{user}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = response.headers()[SET_COOKIE].to_str().unwrap();
    Credential(
        COOKIE.as_str(),
        cookie.split(';').next().unwrap().to_string(),
    )
}

async fn get(app: &Router, credential: Option<&Credential>, uri: &str) -> (StatusCode, Value) {
    let mut request = Request::get(uri);
    if let Some(Credential(name, value)) = credential {
        request = request.header(*name, value);
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

const SEPTEMBER: &str = "from=2026-09-01&to=2026-09-30";

#[sqlx::test]
async fn every_route_reads_only_the_callers_own_usage(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let admin = insert_user(&pool, "admin@example.com", "Admin").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    record(&pool, dev, MACHINE_A, vec![bucket(hour, APP, 1, Some(0.5))]).await;
    record(
        &pool,
        admin,
        MACHINE_B,
        vec![bucket(hour, "github.com/example/admin-only", 7, Some(9.0))],
    )
    .await;
    let app = app(&pool);
    let admin_session = session(&app, admin).await;
    let callers = [
        (token(&pool, dev).await, MACHINE_A, 1, APP),
        (
            token(&pool, admin).await,
            MACHINE_B,
            7,
            "github.com/example/admin-only",
        ),
        (admin_session, MACHINE_B, 7, "github.com/example/admin-only"),
    ];

    for (credential, machine, records, key) in &callers {
        let (status, report) = get(
            &app,
            Some(credential),
            &format!("/ai-usage/report?{SEPTEMBER}"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(report["totals"]["records"], json!(records));
        assert_eq!(report["options"]["machineIds"], json!([machine]));
        assert_eq!(report["projects"][0]["keys"][0]["projectKey"], json!(key));

        // Another user's machine filters to nothing rather than to their usage.
        let other = if *machine == MACHINE_A {
            MACHINE_B
        } else {
            MACHINE_A
        };
        let (_, filtered) = get(
            &app,
            Some(credential),
            &format!("/ai-usage/report?{SEPTEMBER}&machineId={other}"),
        )
        .await;
        assert_eq!(filtered["totals"]["records"], json!(0));

        let (_, sessions) = get(
            &app,
            Some(credential),
            &format!("/ai-usage/sessions?{SEPTEMBER}"),
        )
        .await;
        assert_eq!(sessions["total"], json!(1));
        assert_eq!(sessions["sessions"][0]["machineId"], json!(machine));

        let (_, machines) = get(
            &app,
            Some(credential),
            &format!("/ai-usage/machines?{SEPTEMBER}"),
        )
        .await;
        assert_eq!(machines["machines"].as_array().unwrap().len(), 1);
        assert_eq!(machines["machines"][0]["machineId"], json!(machine));
        assert_eq!(machines["machines"][0]["usage"]["records"], json!(records));

        let (_, keys) = get(&app, Some(credential), "/ai-usage/project-keys").await;
        assert_eq!(keys["projectKeys"].as_array().unwrap().len(), 1);
        assert_eq!(keys["projectKeys"][0]["projectKey"], json!(key));
    }

    for uri in [
        "/ai-usage/report",
        "/ai-usage/sessions",
        "/ai-usage/machines",
        "/ai-usage/project-keys",
    ] {
        assert_eq!(get(&app, None, uri).await.0, StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test]
async fn unknown_costs_are_null_next_to_the_known_subtotal(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let mut other_model = bucket(datetime!(2026-09-22 09:00 UTC), APP, 3, Some(0.25));
    other_model.model = "priced-model".to_string();
    record(
        &pool,
        dev,
        MACHINE_A,
        vec![
            bucket(datetime!(2026-09-22 08:00 UTC), APP, 2, None),
            other_model,
        ],
    )
    .await;
    let app = app(&pool);
    let dev = token(&pool, dev).await;

    let (_, report) = get(&app, Some(&dev), &format!("/ai-usage/report?{SEPTEMBER}")).await;
    assert_eq!(
        report["totals"],
        json!({
            "tokens": { "input": 50, "cacheRead": 500, "cacheWrite": 25, "output": 35 },
            "records": 5,
            "estimatedCostUsd": null,
            "pricedCostUsd": 0.25,
            "unpricedRecords": 2
        })
    );
    let models = report["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| {
            (
                model["model"].clone(),
                model["totals"]["estimatedCostUsd"].clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        models,
        [
            (json!("priced-model"), json!(0.25)),
            (json!("example-model"), Value::Null),
        ]
    );
    assert_eq!(
        report["projects"][0]["keys"][0],
        json!({
            "projectKey": APP,
            "status": "unmapped",
            "project": null,
            "mappable": true,
            "totals": report["totals"],
        })
    );
}

#[sqlx::test]
async fn the_range_defaults_to_the_current_local_month(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let app = app(&pool);
    let dev = token(&pool, dev).await;
    let (first_day, last_day): (Date, Date) = sqlx::query_as(
        "SELECT date_trunc('month', now() AT TIME ZONE 'Europe/Stockholm')::date,
                 (date_trunc('month', now() AT TIME ZONE 'Europe/Stockholm')
                     + interval '1 month' - interval '1 day')::date",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (status, report) = get(&app, Some(&dev), "/ai-usage/report").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["timeZone"], "Europe/Stockholm");
    assert_eq!(report["from"], first_day.to_string());
    assert_eq!(report["to"], last_day.to_string());
    assert_eq!(report["totals"]["estimatedCostUsd"], json!(0.0));
}

#[sqlx::test]
async fn sessions_page_through_next_cursors(pool: PgPool) {
    let user = insert_user(&pool, "dev@example.com", "User").await;
    let app = app(&pool);
    let dev = token(&pool, user).await;

    // Ordinary costs are the control. A large accepted cost must also allow
    // the next request to consume its exact NUMERIC rank from nextCursor.
    for (largest_cost, limit) in [(0.13, 2), (1e308, 1)] {
        let buckets = (8..14)
            .map(|hour| {
                let cost = if hour == 13 {
                    largest_cost
                } else {
                    f64::from(hour) / 100.0
                };
                let mut bucket = bucket(
                    datetime!(2026-09-22 00:00 UTC).replace_hour(hour).unwrap(),
                    APP,
                    1,
                    Some(cost),
                );
                bucket.session_key = format!("{hour:0>32}");
                bucket
            })
            .collect();
        record(&pool, user, MACHINE_A, buckets).await;

        let mut seen = Vec::new();
        let mut uri = format!("/ai-usage/sessions?{SEPTEMBER}&sort=cost&limit={limit}");
        loop {
            let (status, page) = get(&app, Some(&dev), &uri).await;
            assert_eq!(status, StatusCode::OK, "cost {largest_cost}: {page}");
            assert_eq!(page["total"], json!(6));
            for session in page["sessions"].as_array().unwrap() {
                seen.push(session["totals"]["pricedCostUsd"].as_f64().unwrap());
            }
            match page["nextCursor"].as_str() {
                Some(cursor) => {
                    uri = format!(
                        "/ai-usage/sessions?{SEPTEMBER}&sort=cost&limit={limit}&after={cursor}"
                    );
                }
                None => break,
            }
        }
        assert_eq!(seen, [largest_cost, 0.12, 0.11, 0.1, 0.09, 0.08]);
    }

    // A cursor only continues the order it came from.
    let (_, first) = get(
        &app,
        Some(&dev),
        &format!("/ai-usage/sessions?{SEPTEMBER}&sort=cost&limit=2"),
    )
    .await;
    let cursor = first["nextCursor"].as_str().unwrap();
    let (status, _) = get(
        &app,
        Some(&dev),
        &format!("/ai-usage/sessions?{SEPTEMBER}&sort=tokens&after={cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn responses_say_whether_time_tracking_is_configured(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    record(
        &pool,
        dev,
        MACHINE_A,
        vec![bucket(datetime!(2026-09-22 08:00 UTC), APP, 1, Some(0.1))],
    )
    .await;
    let configured = app(&pool);
    let unconfigured = app_with(&pool, None);
    let dev = token(&pool, dev).await;

    for (app, expected) in [(&configured, true), (&unconfigured, false)] {
        let (_, report) = get(app, Some(&dev), &format!("/ai-usage/report?{SEPTEMBER}")).await;
        assert_eq!(report["timeTrackingConfigured"], json!(expected));
        let (_, keys) = get(app, Some(&dev), "/ai-usage/project-keys").await;
        assert_eq!(keys["timeTrackingConfigured"], json!(expected));
        assert_eq!(keys["projectKeys"][0]["mappable"], json!(expected));
    }
}

#[sqlx::test]
async fn malformed_queries_are_bad_requests(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let app = app(&pool);
    let dev = token(&pool, dev).await;

    for uri in [
        "/ai-usage/report?from=2026-09-02&to=2026-09-01",
        "/ai-usage/report?from=2025-01-01&to=2026-09-01",
        "/ai-usage/report?from=22-09-2026",
        "/ai-usage/report?provider=gemini",
        "/ai-usage/report?machineId=laptop",
        "/ai-usage/report?projectId=101&unassigned=true",
        "/ai-usage/sessions?limit=0",
        "/ai-usage/sessions?sort=oldest",
        "/ai-usage/sessions?after=cost~1~2",
        "/ai-usage/machines?from=2026-09-02&to=2026-09-01",
    ] {
        let (status, body) = get(&app, Some(&dev), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert!(body["error"].is_string(), "{uri}: {body}");
    }
}
