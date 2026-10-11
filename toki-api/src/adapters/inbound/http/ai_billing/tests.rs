use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, COOKIE, SET_COOKIE},
        Method, Request, StatusCode,
    },
    routing::post,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    Date, OffsetDateTime,
};
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use crate::{
    adapters::outbound::postgres::{
        PostgresAiBillingRepository, PostgresAiProjectMappingRepository,
        PostgresAiSubscriptionRepository, PostgresAiUsageRepository, PostgresApiTokenRepository,
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{
            AiCoverageStatus, AiCurrency, AiMachine, AiMachineId, AiMonthlyCost, AiPricing,
            AiPricingStatus, AiProjectKey, AiProvider, AiProviderCoverage, AiSubscriptionPeriod,
            AiSubscriptionPlan, AiSubscriptionTerms, AiTokenCounts, AiUsageBucket, AiUsageTimeZone,
            AiUsageUpload, AiUsageWindow, ProjectId, TimeTrackingCompany, UNATTRIBUTED_PROJECT_KEY,
        },
        ports::{
            inbound::{ApiTokenAuthenticator, ApiTokenService},
            outbound::{AiProjectMappingRepository, AiSubscriptionRepository, AiUsageRepository},
        },
        services::{AiBillingServiceImpl, ApiTokenServiceImpl},
    },
};

use super::*;

const APP: &str = "github.com/example/app";
const TOOLS: &str = "github.com/example/tools";
const LAPTOP: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
const DESKTOP: &str = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
const COLLEAGUE_LAPTOP: &str = "7e57c0de-0000-4000-8000-000000000001";
const SESSIONS: [&str; 3] = [
    "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f",
    "0f1e2d3c4b5a69788796a5b4c3d2e1f0",
    "aaaabbbbccccddddeeeeffff00001111",
];

fn company() -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: "company-1".to_string(),
    }
}

fn db(pool: &PgPool) -> crate::db::DbPool {
    sqlx_tracing::PoolBuilder::from(pool.clone()).build()
}

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The routes composed as in production: bearer tokens and browser sessions
/// both authenticate, and a bearer token takes precedence over a cookie.
fn app(pool: &PgPool) -> Router {
    let db = db(pool);
    let service: Arc<dyn AiBillingService> = Arc::new(AiBillingServiceImpl::new(
        Arc::new(PostgresAiBillingRepository::new(db.clone())),
        Arc::new(PostgresAiSubscriptionRepository::new(db.clone())),
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
        Some(company()),
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
        .nest("/ai-usage/admin", router())
        .route_layer(middleware::from_fn(require_authenticated))
        .layer(middleware::from_fn_with_state(tokens, authenticate_bearer))
        .route("/test-sign-in/{user_id}", post(sign_in))
        .with_state(service)
        .layer(AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build())
}

/// How a request authenticates.
enum Caller<'a> {
    Anonymous,
    Token(&'a str),
    Session(&'a str),
    /// An API token and a session cookie together.
    TokenAndSession(&'a str, &'a str),
}

/// Status, headers and body of a GET.
async fn get(app: &Router, caller: &Caller<'_>, uri: &str) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder().method(Method::GET).uri(uri);
    request = match caller {
        Caller::Anonymous => request,
        Caller::Token(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
        Caller::Session(cookie) => request.header(COOKIE, *cookie),
        Caller::TokenAndSession(token, cookie) => request
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(COOKIE, *cookie),
    };
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get_json(app: &Router, caller: &Caller<'_>, uri: &str) -> Value {
    let (status, _, body) = get(app, caller, uri).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    serde_json::from_str(&body).unwrap()
}

use axum::http::HeaderMap;

struct TestUser {
    id: UserId,
    token: String,
    cookie: String,
}

async fn test_user(
    app: &Router,
    pool: &PgPool,
    email: &str,
    full_name: &str,
    roles: &[&str],
) -> TestUser {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ($1, $2, '', '', $3)
         RETURNING id",
    )
    .bind(email)
    .bind(full_name)
    .bind(roles)
    .fetch_one(pool)
    .await
    .unwrap();
    let token = ApiTokenServiceImpl::new(Arc::new(PostgresApiTokenRepository::new(db(pool))))
        .create(&UserId::new(id), "token-ledger")
        .await
        .unwrap()
        .secret
        .as_str()
        .to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/test-sign-in/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cookie = response.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    TestUser {
        id: UserId::new(id),
        token,
        cookie,
    }
}

/// One session's usage of one model in one hour, priced at `cost` dollars.
fn bucket(
    hour_start: OffsetDateTime,
    session: usize,
    project_key: &str,
    provider: AiProvider,
    records: i64,
    cost: Option<f64>,
) -> AiUsageBucket {
    AiUsageBucket {
        hour_start,
        session_key: SESSIONS[session].to_string(),
        project_key: project_key.to_string(),
        provider,
        model: format!("{}-model", provider.as_str()),
        tokens: AiTokenCounts {
            input: 100 * records,
            cache_read: 1_000 * records,
            cache_write: 10 * records,
            output: 50 * records,
        },
        records,
        estimated_cost_usd: cost,
        unpriced_records: if cost.is_none() { records } else { 0 },
    }
}

async fn upload(
    pool: &PgPool,
    user: UserId,
    machine: &str,
    label: &str,
    window: (OffsetDateTime, OffsetDateTime),
    buckets: Vec<AiUsageBucket>,
) {
    let upload = AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(machine).unwrap(),
            label: label.to_string(),
            client_version: "0.2.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        AiUsageWindow::new(window.0, window.1).unwrap(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: None,
            source: "https://example.com/prices.json".to_string(),
        },
        buckets,
        [AiProvider::Claude, AiProvider::Codex]
            .into_iter()
            .map(|provider| AiProviderCoverage {
                provider,
                status: AiCoverageStatus::Ok,
                files: 1,
                unreadable: 0,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 0,
            })
            .collect(),
        Vec::new(),
    )
    .unwrap();
    PostgresAiUsageRepository::new(db(pool))
        .replace_window(&user, &upload)
        .await
        .unwrap();
}

async fn map(pool: &PgPool, key: &str, id: &str, name: &str, by: UserId) {
    PostgresAiProjectMappingRepository::new(db(pool))
        .upsert(
            &AiProjectKey::parse(key).unwrap(),
            &company(),
            &AiMappedProject {
                id: ProjectId::new(id),
                name: name.to_string(),
            },
            &by,
        )
        .await
        .unwrap();
}

/// Declares a subscription with a fee of `(amount, currency)` for an
/// inclusive `(first, last)` period.
async fn subscribe(
    pool: &PgPool,
    user: UserId,
    provider: AiProvider,
    plan: &str,
    (fee, currency): (&str, &str),
    (valid_from, valid_to): (Date, Option<Date>),
) -> i32 {
    PostgresAiSubscriptionRepository::new(db(pool))
        .insert(
            &user,
            &AiSubscriptionTerms {
                provider,
                plan: AiSubscriptionPlan::parse(plan).unwrap(),
                monthly_cost: AiMonthlyCost::parse(fee).unwrap(),
                currency: AiCurrency::parse(currency).unwrap(),
                period: AiSubscriptionPeriod::new(valid_from, valid_to).unwrap(),
            },
        )
        .await
        .unwrap()
        .id
        .as_i32()
}

/// Synced windows that end after August 2026 in Stockholm (2 September).
const AUGUST_WINDOW: (OffsetDateTime, OffsetDateTime) = (
    datetime!(2026-07-31 22:00 UTC),
    datetime!(2026-09-01 22:00 UTC),
);

struct August {
    admin: TestUser,
    dev: TestUser,
    colleague: TestUser,
    max: i32,
    pro: i32,
}

/// August 2026 in Stockholm (UTC+2): a developer with Claude Max from 16
/// August and API usage before it, and a colleague whose ChatGPT Pro went
/// unused. Usage is spread over hours and sessions that admins must not see.
async fn august(app: &Router, pool: &PgPool) -> August {
    let admin = test_user(
        app,
        pool,
        "admin@example.com",
        "Admin Example",
        &["User", "Admin"],
    )
    .await;
    let dev = test_user(app, pool, "dev@example.com", "Dev Example", &["User"]).await;
    let colleague = test_user(
        app,
        pool,
        "colleague@example.com",
        "Colleague Example",
        &["User"],
    )
    .await;
    map(pool, APP, "101", "Client A", admin.id).await;

    use AiProvider::{Claude, Codex};
    upload(
        pool,
        dev.id,
        LAPTOP,
        "work-laptop",
        AUGUST_WINDOW,
        vec![
            // Midnight on 1 August in Stockholm, still July in UTC: August.
            bucket(
                datetime!(2026-07-31 22:00 UTC),
                0,
                APP,
                Claude,
                1,
                Some(0.25),
            ),
            bucket(
                datetime!(2026-08-10 08:00 UTC),
                0,
                APP,
                Claude,
                2,
                Some(0.5),
            ),
            bucket(
                datetime!(2026-08-20 08:00 UTC),
                0,
                APP,
                Claude,
                3,
                Some(1.25),
            ),
            bucket(
                datetime!(2026-08-20 13:00 UTC),
                1,
                APP,
                Claude,
                4,
                Some(1.75),
            ),
            bucket(
                datetime!(2026-08-21 09:00 UTC),
                2,
                UNATTRIBUTED_PROJECT_KEY,
                Claude,
                1,
                Some(1.0),
            ),
            bucket(datetime!(2026-08-05 09:00 UTC), 1, TOOLS, Codex, 5, None),
            // Midnight on 1 September in Stockholm, still August in UTC: September.
            bucket(
                datetime!(2026-08-31 22:00 UTC),
                0,
                APP,
                Claude,
                9,
                Some(9.0),
            ),
        ],
    )
    .await;
    upload(
        pool,
        dev.id,
        DESKTOP,
        "desktop",
        AUGUST_WINDOW,
        vec![bucket(
            datetime!(2026-08-20 15:00 UTC),
            2,
            APP,
            Claude,
            1,
            Some(1.0),
        )],
    )
    .await;
    upload(
        pool,
        colleague.id,
        COLLEAGUE_LAPTOP,
        "colleague-laptop",
        AUGUST_WINDOW,
        Vec::new(),
    )
    .await;

    let max = subscribe(
        pool,
        dev.id,
        Claude,
        "Claude Max 5x",
        ("1100", "SEK"),
        (date!(2026 - 08 - 16), None),
    )
    .await;
    let pro = subscribe(
        pool,
        colleague.id,
        Codex,
        "ChatGPT Pro",
        ("200", "USD"),
        (date!(2026 - 01 - 01), None),
    )
    .await;

    August {
        admin,
        dev,
        colleague,
        max,
        pro,
    }
}

/// A line as `(user, provider, project, mode, overhead, billable amount,
/// currency, usd, unpriced records, records)`.
fn line_summary(line: &Value) -> Value {
    json!([
        line["userId"],
        line["provider"],
        line["project"]["projectName"],
        line["billingMode"],
        line["unallocatedOverhead"],
        line["billableAmount"],
        line["billableCurrency"],
        line["usage"]["apiEquivalentUsd"],
        line["usage"]["unpricedRecords"],
        line["usage"]["records"],
    ])
}

#[sqlx::test]
async fn admin_billing_routes_need_a_plain_admin_session(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(
        &app,
        &pool,
        "admin@example.com",
        "Admin",
        &["User", "Admin"],
    )
    .await;
    let dev = test_user(&app, &pool, "dev@example.com", "Dev", &["User"]).await;
    let routes = [
        "/ai-usage/admin/billing/2026-08".to_string(),
        "/ai-usage/admin/billing/2026-08/completeness".to_string(),
        format!("/ai-usage/admin/billing/2026-08/developers/{}", dev.id),
        "/ai-usage/admin/billing/2026-08/export.csv".to_string(),
        "/ai-usage/admin/developers".to_string(),
    ];

    for caller in [
        Caller::Session(&dev.cookie),
        Caller::Token(&dev.token),
        // API tokens never carry admin power, not even an admin's.
        Caller::Token(&admin.token),
        // A token with an admin's session cookie authenticates as the token.
        Caller::TokenAndSession(&dev.token, &admin.cookie),
        Caller::TokenAndSession(&admin.token, &admin.cookie),
    ] {
        for uri in &routes {
            let (status, _, body) = get(&app, &caller, uri).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
        }
    }
    for uri in &routes {
        let (status, _, _) = get(&app, &Caller::Anonymous, uri).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
        let (status, _, body) = get(&app, &Caller::Session(&admin.cookie), uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
}

#[sqlx::test]
async fn the_overview_bills_subscriptions_by_share_and_api_days_at_cost(pool: PgPool) {
    let app = app(&pool);
    let August {
        admin,
        dev,
        colleague,
        max,
        pro,
    } = august(&app, &pool).await;
    let overview = get_json(
        &app,
        &Caller::Session(&admin.cookie),
        "/ai-usage/admin/billing/2026-08",
    )
    .await;

    assert_eq!(
        (
            &overview["month"],
            &overview["firstDay"],
            &overview["lastDay"],
            &overview["timeZone"]
        ),
        (
            &json!("2026-08"),
            &json!("2026-08-01"),
            &json!("2026-08-31"),
            &json!("Europe/Stockholm")
        )
    );
    let lines: Vec<Value> = overview["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(line_summary)
        .collect();
    // Claude Max covers 16–31 August, 16 of 31 days: 1100 × 16 / 31 =
    // 567.74 SEK, split 4.00 : 1.00 USD between Client A and Unassigned.
    // Earlier Claude days and all Codex days bill as API usage.
    let (dev_id, colleague_id) = (dev.id.as_i32(), colleague.id.as_i32());
    assert_eq!(
        lines,
        [
            json!([
                dev_id,
                "claude",
                "Client A",
                "subscription",
                false,
                "454.19",
                "SEK",
                4.0,
                0,
                8
            ]),
            json!([
                dev_id,
                "claude",
                null,
                "subscription",
                false,
                "113.55",
                "SEK",
                1.0,
                0,
                1
            ]),
            json!([dev_id, "claude", "Client A", "api", false, "0.75", "USD", 0.75, 0, 3]),
            // All unpriced: the amount is unknown, not zero.
            json!([dev_id, "codex", null, "api", false, null, "USD", 0.0, 5, 5]),
            json!([
                colleague_id,
                "codex",
                null,
                "subscription",
                true,
                "200.00",
                "USD",
                0.0,
                0,
                0
            ]),
        ]
    );
    assert_eq!(overview["lines"][0]["subscriptionId"], max);
    assert_eq!(overview["lines"][4]["subscriptionId"], pro);

    let subscriptions = overview["subscriptions"].as_array().unwrap();
    assert_eq!(
        subscriptions[0],
        json!({
            "subscriptionId": max,
            "userId": dev_id,
            "provider": "claude",
            "plan": "Claude Max 5x",
            "monthlyCost": "1100.00",
            "currency": "SEK",
            "validFrom": "2026-08-16",
            "validTo": null,
            "coveredFrom": "2026-08-16",
            "coveredTo": "2026-08-31",
            "coveredDays": 16,
            "daysInMonth": 31,
            "proratedFee": "567.74",
            "allocation": "apiCost",
            "usage": {
                "apiEquivalentUsd": 5.0,
                "unpricedRecords": 0,
                "records": 9,
                "tokens": {
                    "input": 900,
                    "cacheRead": 9000,
                    "cacheWrite": 90,
                    "output": 450,
                    "total": 10440
                }
            }
        })
    );
    assert_eq!(subscriptions[1]["allocation"], "unallocated");
    assert_eq!(subscriptions[1]["proratedFee"], "200.00");

    assert_eq!(
        overview["totals"]["fees"],
        json!([
            { "currency": "SEK", "billed": "567.74", "overhead": "0.00" },
            { "currency": "USD", "billed": "200.00", "overhead": "200.00" },
        ])
    );
    assert_eq!(overview["totals"]["apiBilledUsd"], "0.75");
    assert_eq!(overview["totals"]["apiUnpricedRecords"], 5);
    let developers: Vec<&Value> = overview["developers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|developer| &developer["fullName"])
        .collect();
    assert_eq!(
        developers,
        [&json!("Colleague Example"), &json!("Dev Example")]
    );
}

/// Every object key in a JSON document, recursively.
fn keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                found.push(key.clone());
                keys(value, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
        _ => {}
    }
}

/// Every string value in a JSON document, recursively.
fn strings(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) => found.push(text.clone()),
        Value::Object(object) => object.values().for_each(|value| strings(value, found)),
        Value::Array(items) => items.iter().for_each(|item| strings(item, found)),
        _ => {}
    }
}

#[sqlx::test]
async fn admin_responses_never_expose_hours_or_sessions(pool: PgPool) {
    let app = app(&pool);
    let August { admin, dev, .. } = august(&app, &pool).await;
    let admin = Caller::Session(&admin.cookie);

    let mut documents = Vec::new();
    for uri in [
        "/ai-usage/admin/billing/2026-08".to_string(),
        "/ai-usage/admin/billing/2026-08/completeness".to_string(),
        format!("/ai-usage/admin/billing/2026-08/developers/{}", dev.id),
        "/ai-usage/admin/developers".to_string(),
    ] {
        documents.push((uri.clone(), get_json(&app, &admin, &uri).await));
    }

    // A time of day in any form, such as an RFC 3339 instant.
    let time_of_day = regex::Regex::new(r"\d{1,2}:\d{2}").unwrap();
    for (uri, document) in &documents {
        let mut found = Vec::new();
        keys(document, &mut found);
        for key in found {
            let lower = key.to_ascii_lowercase();
            assert!(
                !lower.contains("hour") && !lower.contains("session") && !key.ends_with("At"),
                "{uri} exposes {key:?}"
            );
        }
        let mut values = Vec::new();
        strings(document, &mut values);
        for value in values {
            let session_like =
                value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
            assert!(!session_like, "{uri} exposes a session key {value:?}");
            // Days only: not even a last sync carries its time.
            assert!(
                !time_of_day.is_match(&value),
                "{uri} exposes a time of day {value:?}"
            );
        }
    }
    let completeness = &documents[1].1;
    assert_eq!(
        completeness["developers"][0]["machines"][0]["lastSyncedOn"]
            .as_str()
            .map(str::len),
        Some("2026-09-02".len())
    );

    let (_, _, csv) = get(&app, &admin, "/ai-usage/admin/billing/2026-08/export.csv").await;
    for session in SESSIONS {
        assert!(!csv.contains(session));
    }
    assert!(!time_of_day.is_match(&csv), "{csv}");
    let header = csv.lines().next().unwrap().to_ascii_lowercase();
    assert!(!header.contains("hour") && !header.contains("session"));

    // The drill-down sums hours and sessions into days: 20 August has three
    // sessions over three hours on two machines.
    let (_, drill_down) = &documents[2];
    let day: Vec<(&Value, &Value, &Value)> = drill_down["dailyUsage"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["day"] == "2026-08-20")
        .map(|row| {
            (
                &row["machineId"],
                &row["usage"]["records"],
                &row["subscriptionId"],
            )
        })
        .collect();
    assert_eq!(day.len(), 2);
    assert!(day.contains(&(
        &json!(LAPTOP),
        &json!(7),
        &drill_down["subscriptions"][0]["subscriptionId"]
    )));
    assert!(day.contains(&(
        &json!(DESKTOP),
        &json!(1),
        &drill_down["subscriptions"][0]["subscriptionId"]
    )));
}

#[sqlx::test]
async fn the_drill_down_shows_a_developers_days_by_provider_model_machine_and_project(
    pool: PgPool,
) {
    let app = app(&pool);
    let August {
        admin, dev, max, ..
    } = august(&app, &pool).await;
    let drill_down = get_json(
        &app,
        &Caller::Session(&admin.cookie),
        &format!("/ai-usage/admin/billing/2026-08/developers/{}", dev.id),
    )
    .await;

    assert_eq!(drill_down["developer"]["fullName"], "Dev Example");
    assert_eq!(
        drill_down["machines"],
        json!([
            { "machineId": DESKTOP, "label": "desktop" },
            { "machineId": LAPTOP, "label": "work-laptop" },
        ])
    );
    let rows: Vec<Value> = drill_down["dailyUsage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!([
                row["day"],
                row["provider"],
                row["model"],
                row["machineId"],
                row["projectKey"],
                row["project"]["projectName"],
                row["subscriptionId"],
                row["usage"]["records"],
            ])
        })
        .collect();
    assert_eq!(
        rows,
        [
            json!([
                "2026-08-01",
                "claude",
                "claude-model",
                LAPTOP,
                APP,
                "Client A",
                null,
                1
            ]),
            json!([
                "2026-08-05",
                "codex",
                "codex-model",
                LAPTOP,
                TOOLS,
                null,
                null,
                5
            ]),
            json!([
                "2026-08-10",
                "claude",
                "claude-model",
                LAPTOP,
                APP,
                "Client A",
                null,
                2
            ]),
            json!([
                "2026-08-20",
                "claude",
                "claude-model",
                DESKTOP,
                APP,
                "Client A",
                max,
                1
            ]),
            json!([
                "2026-08-20",
                "claude",
                "claude-model",
                LAPTOP,
                APP,
                "Client A",
                max,
                7
            ]),
            json!([
                "2026-08-21",
                "claude",
                "claude-model",
                LAPTOP,
                UNATTRIBUTED_PROJECT_KEY,
                null,
                max,
                1
            ]),
        ]
    );
    // The developer's own lines, as in the overview.
    assert_eq!(drill_down["lines"].as_array().unwrap().len(), 4);
    assert_eq!(drill_down["subscriptions"][0]["proratedFee"], "567.74");

    let (status, _, _) = get(
        &app,
        &Caller::Session(&admin.cookie),
        "/ai-usage/admin/billing/2026-08/developers/999999",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn unsupported_billing_totals_return_a_range_error_instead_of_a_server_failure(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(
        &app,
        &pool,
        "admin@example.com",
        "Admin Example",
        &["Admin"],
    )
    .await;
    let caller = Caller::Session(&admin.cookie);
    let month = "/ai-usage/admin/billing/2026-09";
    assert_eq!(get(&app, &caller, month).await.0, StatusCode::OK);
    let excessive_cost = bucket(
        datetime!(2026-09-01 00:00 UTC),
        0,
        APP,
        AiProvider::Claude,
        1,
        Some(10_000_000_000.0),
    );
    let mut excessive_tokens = bucket(
        datetime!(2026-09-01 00:00 UTC),
        0,
        APP,
        AiProvider::Claude,
        1,
        Some(0.0),
    );
    excessive_tokens.tokens = AiTokenCounts {
        input: 9_007_199_254_740_991,
        output: 1,
        ..AiTokenCounts::default()
    };
    for bucket in [excessive_cost, excessive_tokens] {
        upload(
            &pool,
            admin.id,
            LAPTOP,
            "synthetic-laptop",
            (
                datetime!(2026-09-01 00:00 UTC),
                datetime!(2026-09-02 00:00 UTC),
            ),
            vec![bucket],
        )
        .await;
        for uri in [month.to_string(), format!("{month}/export.csv")] {
            let (status, _, body) = get(&app, &caller, &uri).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}: {body}");
        }
    }
}

#[sqlx::test]
async fn malformed_months_and_users_are_bad_requests(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(
        &app,
        &pool,
        "admin@example.com",
        "Admin",
        &["User", "Admin"],
    )
    .await;
    for uri in [
        "/ai-usage/admin/billing/2026-13",
        "/ai-usage/admin/billing/august",
        "/ai-usage/admin/billing/2026-8/completeness",
        "/ai-usage/admin/billing/2026-08-01/export.csv",
        "/ai-usage/admin/billing/2026-08/developers/dev",
    ] {
        let (status, _, body) = get(&app, &Caller::Session(&admin.cookie), uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert!(body["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()));
    }
}

/// Parses CSV that quotes every cell, as RFC 4180 reads it; panics on an
/// unquoted cell.
fn parse_quoted_csv(csv: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut chars = csv.chars().peekable();
    while chars.peek().is_some() {
        assert_eq!(chars.next(), Some('"'), "every cell is quoted: {csv}");
        let mut cell = String::new();
        loop {
            match chars.next() {
                Some('"') if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                Some('"') => break,
                Some(character) => cell.push(character),
                None => panic!("unterminated cell in {csv}"),
            }
        }
        row.push(cell);
        match chars.next() {
            Some(',') => {}
            Some('\r') => {
                assert_eq!(chars.next(), Some('\n'), "rows end with CRLF");
                rows.push(std::mem::take(&mut row));
            }
            other => panic!("unexpected {other:?} after a cell in {csv}"),
        }
    }
    assert!(row.is_empty(), "the last row ends with CRLF");
    rows
}

#[sqlx::test]
async fn the_csv_export_bills_per_project_and_guards_against_formulas(pool: PgPool) {
    let app = app(&pool);
    let August {
        admin, dev, pro, ..
    } = august(&app, &pool).await;
    // Text that a spreadsheet would run as a formula, or split into cells
    // and then run: Swedish-locale Excel splits on semicolons.
    map(
        &pool,
        APP,
        "101",
        "=HYPERLINK(\"https://example.com\")",
        admin.id,
    )
    .await;
    map(&pool, TOOLS, "202", "Tools\t-internal", admin.id).await;
    sqlx::query("UPDATE users SET full_name = '@Dev, \"Example\"' WHERE id = $1")
        .bind(dev.id.as_i32())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE ai_subscriptions SET plan = $2 WHERE id = $1")
        .bind(pro)
        .bind("Pro;=cmd|' /C calc'!A0")
        .execute(&pool)
        .await
        .unwrap();

    let (status, headers, csv) = get(
        &app,
        &Caller::Session(&admin.cookie),
        "/ai-usage/admin/billing/2026-08/export.csv",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "text/csv; charset=utf-8");
    assert_eq!(
        headers[CONTENT_DISPOSITION],
        "attachment; filename=\"ai-usage-billing-2026-08.csv\""
    );
    let csv = csv
        .strip_prefix('\u{feff}')
        .expect("a UTF-8 byte order mark");
    // Split on semicolons, as Swedish Excel does, no piece may start a formula.
    for piece in csv.split([';', ',', '\t', '\r', '\n']) {
        let text = piece.trim_start_matches('"');
        assert!(
            !text.starts_with(['=', '+', '-', '@']),
            "{piece:?} would run as a formula"
        );
    }
    assert!(csv.contains("\"Pro;'=cmd|' /C calc'!A0\""), "{csv}");

    let dev = "'@Dev, \"Example\"";
    let link = "'=HYPERLINK(\"https://example.com\")";
    let rows = parse_quoted_csv(csv);
    let expected: Vec<Vec<&str>> = vec![
        vec![
            "month",
            "project_id",
            "project",
            "developer",
            "provider",
            "billing_mode",
            "plan",
            "billable_amount",
            "billable_currency",
            "api_equivalent_usd",
            "unpriced_records",
            "input_tokens",
            "cache_read_tokens",
            "cache_write_tokens",
            "output_tokens",
            "total_tokens",
            "allocation_basis",
            "warning",
        ],
        vec![
            "2026-08",
            "101",
            link,
            dev,
            "claude",
            "subscription",
            "Claude Max 5x",
            "454.19",
            "SEK",
            "4.0000",
            "0",
            "800",
            "8000",
            "80",
            "400",
            "9280",
            "api_cost",
            "",
        ],
        vec![
            "2026-08", "101", link, dev, "claude", "api", "", "0.75", "USD", "0.7500", "0", "300",
            "3000", "30", "150", "3480", "", "",
        ],
        vec![
            "2026-08",
            "202",
            "Tools\t'-internal",
            dev,
            "codex",
            "api",
            "",
            "",
            "USD",
            "",
            "5",
            "500",
            "5000",
            "50",
            "250",
            "5800",
            "",
            "5 unpriced records: their cost is unknown and not billed",
        ],
        vec![
            "2026-08",
            "",
            "Unassigned",
            dev,
            "claude",
            "subscription",
            "Claude Max 5x",
            "113.55",
            "SEK",
            "1.0000",
            "0",
            "100",
            "1000",
            "10",
            "50",
            "1160",
            "api_cost",
            "",
        ],
        vec![
            "2026-08",
            "",
            "Unallocated overhead",
            "Colleague Example",
            "codex",
            "subscription",
            "Pro;'=cmd|' /C calc'!A0",
            "200.00",
            "USD",
            "0.0000",
            "0",
            "0",
            "0",
            "0",
            "0",
            "0",
            "none",
            "no usage on the days the subscription covers",
        ],
    ];
    assert_eq!(rows, expected);
}

#[sqlx::test]
async fn api_amounts_bill_whole_cents_and_the_csv_adds_up_to_the_totals(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(
        &app,
        &pool,
        "admin@example.com",
        "Admin",
        &["User", "Admin"],
    )
    .await;
    let dev = test_user(&app, &pool, "dev@example.com", "Dev", &["User"]).await;
    map(&pool, APP, "101", "Client A", admin.id).await;
    map(&pool, TOOLS, "202", "Client B", admin.id).await;
    let hour = datetime!(2026-08-12 08:00 UTC);
    use AiProvider::Claude;
    upload(
        &pool,
        dev.id,
        LAPTOP,
        "laptop",
        AUGUST_WINDOW,
        vec![
            bucket(hour, 0, APP, Claude, 1, Some(0.004)),
            bucket(hour, 0, TOOLS, Claude, 1, Some(0.004)),
            bucket(hour, 0, UNATTRIBUTED_PROJECT_KEY, Claude, 1, Some(0.004)),
        ],
    )
    .await;
    let admin = Caller::Session(&admin.cookie);

    let overview = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08").await;
    let billed: Vec<&Value> = overview["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| &line["billableAmount"])
        .collect();
    assert_eq!(billed, [&json!("0.00"), &json!("0.00"), &json!("0.00")]);
    assert_eq!(overview["totals"]["apiBilledUsd"], "0.00");
    let estimate = overview["totals"]["usage"]["apiEquivalentUsd"]
        .as_f64()
        .unwrap();
    assert!((estimate - 0.012).abs() < 1e-12, "{estimate}");

    let (_, _, csv) = get(&app, &admin, "/ai-usage/admin/billing/2026-08/export.csv").await;
    let rows = parse_quoted_csv(csv.strip_prefix('\u{feff}').unwrap());
    let cents: i64 = rows[1..]
        .iter()
        .map(|row| {
            assert_eq!(row[8], "USD");
            row[7].replace('.', "").parse::<i64>().unwrap()
        })
        .sum();
    assert_eq!(cents, 0);
    assert!(rows[1..].iter().all(|row| row[9] == "0.0040"));
}

#[sqlx::test]
async fn the_drill_down_bills_exactly_as_the_overview(pool: PgPool) {
    // Costs below a billionth of a dollar: summed per day and project,
    // Client A weighs 0.6 + 0.6 → 1 and Client B 1.5 → 2 billionths; summed
    // per model first, Client A would weigh 1 + 1 = 2 and win the tie.
    let app = app(&pool);
    let admin = test_user(
        &app,
        &pool,
        "admin@example.com",
        "Admin",
        &["User", "Admin"],
    )
    .await;
    let dev = test_user(&app, &pool, "dev@example.com", "Dev", &["User"]).await;
    map(&pool, APP, "101", "Client A", admin.id).await;
    map(&pool, TOOLS, "202", "Client B", admin.id).await;
    let hour = datetime!(2026-08-12 08:00 UTC);
    let tiny = |key: &str, model: &str, cost: f64| AiUsageBucket {
        model: model.to_string(),
        ..bucket(hour, 0, key, AiProvider::Claude, 1, Some(cost))
    };
    upload(
        &pool,
        dev.id,
        LAPTOP,
        "laptop",
        AUGUST_WINDOW,
        vec![
            tiny(APP, "model-a", 0.000_000_000_6),
            tiny(APP, "model-b", 0.000_000_000_6),
            tiny(TOOLS, "model-a", 0.000_000_001_5),
        ],
    )
    .await;
    subscribe(
        &pool,
        dev.id,
        AiProvider::Claude,
        "Claude Pro",
        ("0.01", "SEK"),
        (date!(2026 - 08 - 01), None),
    )
    .await;
    let admin = Caller::Session(&admin.cookie);

    let overview = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08").await;
    let drill_down = get_json(
        &app,
        &admin,
        &format!("/ai-usage/admin/billing/2026-08/developers/{}", dev.id),
    )
    .await;

    let own: Vec<&Value> = overview["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["userId"] == dev.id.as_i32())
        .collect();
    let shares: Vec<(&Value, &Value)> = own
        .iter()
        .map(|line| (&line["project"]["projectName"], &line["billableAmount"]))
        .collect();
    assert_eq!(
        shares,
        [
            (&json!("Client A"), &json!("0.00")),
            (&json!("Client B"), &json!("0.01"))
        ]
    );
    assert_eq!(
        drill_down["lines"]
            .as_array()
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        own
    );
    assert_eq!(drill_down["totals"], overview["totals"]);
    assert_eq!(drill_down["subscriptions"], overview["subscriptions"]);
}

#[test]
fn csv_cells_are_quoted_and_never_start_a_formula() {
    for (value, cell) in [
        ("plain", r#""plain""#),
        ("", r#""""#),
        ("12.50", r#""12.50""#),
        ("Client A, AB", r#""Client A, AB""#),
        (r#"say "hi""#, r#""say ""hi""""#),
        ("two\nlines", "\"two\nlines\""),
        ("=1+1", r#""'=1+1""#),
        ("+46 70", r#""'+46 70""#),
        ("-5", r#""'-5""#),
        ("@SUM(A1)", r#""'@SUM(A1)""#),
        ("\tindented", "\"'\tindented\""),
        ("\rreturn", "\"'\rreturn\""),
        ("a=b", r#""a=b""#),
        // A formula after a separator, for programs that split the cell.
        ("Pro;=cmd|' /C calc'!A0", r#""Pro;'=cmd|' /C calc'!A0""#),
        ("Pro,+1", r#""Pro,'+1""#),
        ("Pro\t@x", "\"Pro\t'@x\""),
        ("Pro\n-1", "\"Pro\n'-1\""),
        ("x;;=1", r#""x;;'=1""#),
        ("=a;=b", r#""'=a;'=b""#),
        ("a;b", r#""a;b""#),
    ] {
        assert_eq!(csv_cell(value), cell, "{value:?}");
    }
}

/// Replaces a machine's logged syncs with `provider` intervals, each
/// reported when it ends.
async fn log_syncs(
    pool: &PgPool,
    machine: &str,
    provider: &str,
    intervals: &[(OffsetDateTime, OffsetDateTime)],
) {
    sqlx::query("DELETE FROM ai_usage_coverage_log WHERE machine_id = $1::uuid")
        .bind(machine)
        .execute(pool)
        .await
        .unwrap();
    for (start, end) in intervals {
        sqlx::query(
            "INSERT INTO ai_usage_coverage_log
                 (machine_id, provider, window_start, covered_until, reported_at)
             VALUES ($1::uuid, $2, $3, $4, $4)",
        )
        .bind(machine)
        .bind(provider)
        .bind(start)
        .bind(end)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn set_last_sync(pool: &PgPool, machine: &str, at: OffsetDateTime) {
    sqlx::query("UPDATE ai_usage_machines SET last_synced_at = $2 WHERE id = $1::uuid")
        .bind(machine)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn completeness_flags_machines_that_have_not_uploaded_the_whole_month(pool: PgPool) {
    let app = app(&pool);
    let August { admin, dev, .. } = august(&app, &pool).await;
    let user = |email, name| test_user(&app, &pool, email, name, &["User"]);
    let late = user("late@example.com", "Late Example").await;
    let subscribed = user("subscribed@example.com", "Subscribed Example").await;
    let idle = user("idle@example.com", "Idle Example").await;
    user("other@example.com", "Other Example").await;

    // The desktop synced at 10:00 on 5 August, and next on 3 September
    // with a 14-day window: 5 to 19 August were never uploaded.
    log_syncs(
        &pool,
        DESKTOP,
        "claude",
        &[
            (
                datetime!(2026-07-24 22:00 UTC),
                datetime!(2026-08-05 08:00 UTC),
            ),
            (
                datetime!(2026-08-19 22:00 UTC),
                datetime!(2026-09-02 22:00 UTC),
            ),
        ],
    )
    .await;
    set_last_sync(&pool, DESKTOP, datetime!(2026-09-03 06:00 UTC)).await;
    // Synced at 10:00 on 31 August: its window ends at midnight, but its
    // upload cannot hold that afternoon.
    let late_laptop = "11111111-2222-4333-8444-555555555555";
    let hour = datetime!(2026-08-12 08:00 UTC);
    upload(
        &pool,
        late.id,
        late_laptop,
        "late-laptop",
        AUGUST_WINDOW,
        vec![bucket(hour, 0, APP, AiProvider::Claude, 1, Some(0.1))],
    )
    .await;
    log_syncs(
        &pool,
        late_laptop,
        "claude",
        &[(
            datetime!(2026-07-31 22:00 UTC),
            datetime!(2026-08-31 08:00 UTC),
        )],
    )
    .await;
    // A laptop that last synced in July was not used in August, as far as
    // anyone can tell.
    let old_laptop = "22222222-3333-4444-8555-666666666666";
    upload(
        &pool,
        idle.id,
        old_laptop,
        "old-laptop",
        (
            datetime!(2026-07-01 22:00 UTC),
            datetime!(2026-07-10 22:00 UTC),
        ),
        Vec::new(),
    )
    .await;
    set_last_sync(&pool, old_laptop, datetime!(2026-07-11 06:00 UTC)).await;
    subscribe(
        &pool,
        subscribed.id,
        AiProvider::Claude,
        "Claude Pro",
        ("220", "SEK"),
        (date!(2026 - 08 - 01), Some(date!(2026 - 08 - 31))),
    )
    .await;

    let completeness = get_json(
        &app,
        &Caller::Session(&admin.cookie),
        "/ai-usage/admin/billing/2026-08/completeness",
    )
    .await;

    assert_eq!(completeness["requiredThrough"], "2026-08-31");
    assert_eq!(completeness["inProgress"], false);
    assert_eq!(completeness["staleAfterDays"], 7);
    let developers: Vec<Value> = completeness["developers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|developer| {
            json!([
                developer["fullName"],
                developer["readiness"],
                developer["hasUsage"],
                developer["hasSubscription"],
                developer["machines"].as_array().unwrap().len(),
            ])
        })
        .collect();
    // Users without machines or an August subscription are not listed.
    assert_eq!(
        developers,
        [
            json!(["Late Example", "noSync", true, false, 1]),
            json!(["Subscribed Example", "noSync", false, true, 0]),
            json!(["Dev Example", "incomplete", true, true, 2]),
            json!(["Colleague Example", "ready", false, true, 1]),
            json!(["Idle Example", "noActivity", false, false, 1]),
        ]
    );

    let dev_machines = &completeness["developers"][2]["machines"];
    assert_eq!(completeness["developers"][2]["userId"], dev.id.as_i32());
    assert_eq!(
        dev_machines[0],
        json!({
            "machineId": DESKTOP,
            "label": "desktop",
            "clientVersion": "0.2.0",
            "lastSyncedOn": "2026-09-03",
            "stale": true,
            "activeInMonth": true,
            "complete": false,
            "gaps": [{ "from": "2026-08-05", "to": "2026-08-19" }],
            "providers": [
                {
                    "provider": "codex",
                    "status": "ok",
                    "pricingStatus": "fresh",
                    "expected": false,
                    "gaps": [],
                },
                {
                    "provider": "claude",
                    "status": "ok",
                    "pricingStatus": "fresh",
                    "expected": true,
                    "gaps": [{ "from": "2026-08-05", "to": "2026-08-19" }],
                },
            ],
        })
    );
    assert_eq!(
        (&dev_machines[1]["label"], &dev_machines[1]["complete"]),
        (&json!("work-laptop"), &json!(true))
    );
    let late_machine = &completeness["developers"][0]["machines"][0];
    assert_eq!(
        late_machine["gaps"],
        json!([{ "from": "2026-08-31", "to": "2026-08-31" }])
    );
    let old = &completeness["developers"][4]["machines"][0];
    assert_eq!(
        (&old["lastSyncedOn"], &old["stale"], &old["activeInMonth"]),
        (&json!("2026-07-11"), &json!(true), &json!(false))
    );
}
