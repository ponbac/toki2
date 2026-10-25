use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Mutex},
};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE},
        Method, Request,
    },
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::{
        inbound::http::agent_openapi,
        outbound::postgres::{
            PostgresAiMeteredUsageRepository, PostgresAiProviderAccountRepository,
            PostgresApiTokenRepository,
        },
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{
            merge_credits, utc_day, AiMeteredAmounts, AiMeteredCredit, AiMeteredModel,
            AiMeteredOrgCharge, AiMeteredPeriod, AiMeteredScope, AiMeteredSummaryLine, AiProvider,
        },
        ports::{
            inbound::{ApiTokenAuthenticator, ApiTokenService},
            outbound::MeteredUsageSource,
        },
        services::{
            AiMeteredImportSettings, AiMeteredSourceState, AiMeteredUsageServiceImpl,
            ApiTokenServiceImpl,
        },
    },
};

fn db(pool: &PgPool) -> crate::db::DbPool {
    crate::db::DbPool::from(pool.clone())
}

/// Credits rows per login (`None` is the organization) and UTC day.
type DayRows = BTreeMap<(Option<String>, Date), Vec<AiMeteredCredit>>;

/// Usage per login and UTC day; month totals are their sums. While `held`,
/// every call waits.
#[derive(Default)]
struct FakeSource {
    days: Mutex<DayRows>,
    held: AtomicBool,
    released: tokio::sync::Notify,
}

impl FakeSource {
    async fn wait(&self) {
        while self.held.load(std::sync::atomic::Ordering::SeqCst) {
            self.released.notified().await;
        }
    }

    fn release(&self) {
        self.held.store(false, std::sync::atomic::Ordering::SeqCst);
        self.released.notify_waiters();
    }
}

#[async_trait]
impl MeteredUsageSource for FakeSource {
    async fn credits(
        &self,
        scope: AiMeteredScope<'_>,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredCredit>, MeteredUsageSourceError> {
        self.wait().await;
        let owner = match scope {
            AiMeteredScope::Account(login) => Some(login.as_str().to_string()),
            AiMeteredScope::Organization => None,
        };
        let days = self.days.lock().unwrap();
        let rows: Vec<AiMeteredCredit> = days
            .iter()
            .filter(|((who, day), _)| {
                *who == owner
                    && match period {
                        AiMeteredPeriod::Month(month) => month.contains(*day),
                        AiMeteredPeriod::Day(requested) => *day == requested,
                    }
            })
            .flat_map(|(_, rows)| rows.clone())
            .collect();
        Ok(merge_credits(rows).unwrap())
    }

    async fn org_usage(
        &self,
        _: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredOrgCharge>, MeteredUsageSourceError> {
        self.wait().await;
        Ok(Vec::new())
    }

    async fn summary(
        &self,
        _: AiBillingMonth,
    ) -> Result<Vec<AiMeteredSummaryLine>, MeteredUsageSourceError> {
        self.wait().await;
        Ok(Vec::new())
    }
}

fn sol(quantity: &str) -> AiMeteredCredit {
    let quantity = AiDecimal::parse(quantity).unwrap();
    AiMeteredCredit {
        product: "Copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "ai-credits".to_string(),
        model: AiMeteredModel {
            label: "GPT-6.1 Sol".to_string(),
            model: "gpt-6.1-sol".to_string(),
            auto_routed: false,
        },
        kind: AiCreditKind::Trackable,
        amounts: AiMeteredAmounts {
            unit_price: AiDecimal::parse("0.01").unwrap(),
            gross_quantity: quantity,
            gross_amount: quantity,
            ..AiMeteredAmounts::default()
        },
    }
}

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

fn today() -> Date {
    utc_day(OffsetDateTime::now_utc())
}

fn this_month() -> AiBillingMonth {
    AiBillingMonth::containing(today()).unwrap()
}

/// The routes composed as in production over `source`, importing from the
/// first day of this UTC month.
fn app(pool: &PgPool, source: AiMeteredSourceState) -> Router {
    let db = db(pool);
    let service: Arc<dyn AiMeteredUsageService> = Arc::new(AiMeteredUsageServiceImpl::new(
        Arc::new(PostgresAiMeteredUsageRepository::new(db.clone())),
        Arc::new(PostgresAiProviderAccountRepository::new(db.clone())),
        source,
        AiProvider::Copilot,
        "example-org".to_string(),
        AiMeteredImportSettings {
            backfill_from: this_month().first_day(),
            reprice_floor: time::macros::datetime!(2026-10-01 00:00 UTC),
            unattended: false,
        },
        None,
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

enum Caller<'a> {
    Anonymous,
    Token(&'a str),
    Session(&'a str),
}

async fn send(
    app: &Router,
    caller: &Caller<'_>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    request = match caller {
        Caller::Anonymous => request,
        Caller::Token(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
        Caller::Session(cookie) => request.header(COOKIE, *cookie),
    };
    let body = match body {
        Some(body) => {
            request = request.header(CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)))
    };
    (status, json)
}

struct TestUser {
    id: i32,
    token: String,
    cookie: String,
}

async fn test_user(app: &Router, pool: &PgPool, email: &str, roles: &[&str]) -> TestUser {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ($1, 'Test Person', '', '', $2)
         RETURNING id",
    )
    .bind(email)
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
    TestUser { id, token, cookie }
}

async fn insert_account(pool: &PgPool, id: i64, login: &str) {
    sqlx::query(
        "INSERT INTO ai_provider_accounts (provider, account_id, login) VALUES ('copilot', $1, $2)",
    )
    .bind(id)
    .bind(login)
    .execute(pool)
    .await
    .unwrap();
}

/// Polls the month until no run is running.
async fn wait_for_the_run(app: &Router, caller: &Caller<'_>, uri: &str) -> Value {
    for _ in 0..200 {
        let (status, body) = send(app, caller, Method::GET, uri, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        if body["import"]["running"].is_null() && !body["import"]["lastRun"].is_null() {
            return body;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the import did not finish");
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[sqlx::test]
async fn only_admins_in_a_browser_session_reach_metered_usage(pool: PgPool) {
    let app = app(&pool, AiMeteredSourceState::Disabled);
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let uri = format!("/ai-usage/admin/metered-usage/{}", this_month());
    for (caller, expected) in [
        (Caller::Anonymous, StatusCode::UNAUTHORIZED),
        (Caller::Session(&dev.cookie), StatusCode::FORBIDDEN),
        (Caller::Token(&root.token), StatusCode::FORBIDDEN),
        (Caller::Session(&root.cookie), StatusCode::OK),
    ] {
        let (status, body) = send(&app, &caller, Method::GET, &uri, None).await;
        assert_eq!(status, expected, "{body}");
    }
    let (status, _) = send(
        &app,
        &Caller::Token(&root.token),
        Method::POST,
        &format!("{uri}/import"),
        Some(json!({"mode": "changes"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let session = Caller::Session(&root.cookie);
    let (status, body) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/metered-usage/2026-13",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, _) = send(
        &app,
        &session,
        Method::GET,
        &format!("{uri}/developers/999999"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = send(
        &app,
        &session,
        Method::GET,
        &format!("{uri}/developers/{}", dev.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["residualDays"], json!([]));
    assert_eq!(body["orgCheck"], Value::Null);
    assert_eq!(body["pool"], Value::Null);

    // Off: imports are refused, the month still shows.
    let (status, body) = send(
        &app,
        &session,
        Method::POST,
        &format!("{uri}/import"),
        Some(json!({"mode": "changes"})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let (_, month) = send(&app, &session, Method::GET, &uri, None).await;
    assert_eq!(month["import"]["status"], "disabled");
    assert_eq!(month["import"]["freshness"], "off");

    // Misconfigured: the same, with the reason.
    let misconfigured = self::app(
        &pool,
        AiMeteredSourceState::Misconfigured(
            "github.app_private_key is not a PEM-encoded key".to_string(),
        ),
    );
    let signed_in = misconfigured
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/test-sign-in/{}", root.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = signed_in.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let session = Caller::Session(&cookie);
    let (status, _) = send(
        &misconfigured,
        &session,
        Method::POST,
        &format!("{uri}/import"),
        Some(json!({"mode": "full"})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let (_, month) = send(&misconfigured, &session, Method::GET, &uri, None).await;
    assert_eq!(month["import"]["status"], "misconfigured");
    assert!(month["import"]["message"]
        .as_str()
        .unwrap()
        .contains("github.app_private_key"));
}

#[sqlx::test]
async fn an_import_runs_in_the_background_and_fills_the_month(pool: PgPool) {
    let source = Arc::new(FakeSource::default());
    let app = app(&pool, AiMeteredSourceState::Ready(source.clone()));
    let root = test_user(&app, &pool, "root@example.com", &["Admin"]).await;
    insert_account(&pool, 1001, "ada-lovelace").await;
    source.days.lock().unwrap().insert(
        (Some("ada-lovelace".to_string()), today()),
        vec![sol("12.345678912")],
    );
    source
        .days
        .lock()
        .unwrap()
        .insert((None, today()), vec![sol("17.345678912")]);
    let session = Caller::Session(&root.cookie);
    let uri = format!("/ai-usage/admin/metered-usage/{}", this_month());

    // A body is required, with a mode.
    for body in [None, Some(json!({})), Some(json!({"mode": "everything"}))] {
        let (status, response) =
            send(&app, &session, Method::POST, &format!("{uri}/import"), body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    }
    // Before the backfill.
    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/metered-usage/2020-01/import",
        Some(json!({"mode": "full"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    source.held.store(true, std::sync::atomic::Ordering::SeqCst);
    let (status, started) = send(
        &app,
        &session,
        Method::POST,
        &format!("{uri}/import"),
        Some(json!({"mode": "changes"})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["months"], json!([this_month().to_string()]));
    assert!(started["runId"].is_i64());
    // While it runs: a second import is refused and the run shows.
    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        &format!("{uri}/import"),
        Some(json!({"mode": "full"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (_, running) = send(&app, &session, Method::GET, &uri, None).await;
    assert_eq!(running["import"]["running"]["status"], "running");
    assert_eq!(running["import"]["running"]["kind"], "admin");
    source.release();

    let month = wait_for_the_run(&app, &session, &uri).await;
    assert_eq!(
        keys(&month),
        [
            "accounts",
            "codeReview",
            "import",
            "month",
            "months",
            "namespace",
            "orgCheck",
            "pool",
            "provider",
            "repositories",
            "residualDays",
            "today",
            "totals",
            "unlinkedTracked"
        ]
    );
    assert_eq!(
        keys(&month["import"]),
        [
            "accountErrors",
            "backfillFrom",
            "freshness",
            "lastRun",
            "message",
            "running",
            "sources",
            "staleReason",
            "status",
            "unattended"
        ]
    );
    assert_eq!(month["provider"], "copilot");
    assert_eq!(month["namespace"], "example-org");
    assert_eq!(month["months"], json!([this_month().to_string()]));
    assert_eq!(month["import"]["lastRun"]["status"], "succeeded");
    assert_eq!(month["import"]["sources"].as_array().unwrap().len(), 4);
    let ada = &month["accounts"][0];
    assert_eq!(ada["login"], "ada-lovelace");
    // 12.345678912 credits: hundredths of a credit, and the exact quantity.
    assert_eq!(ada["gross"], "12.35");
    assert_eq!(ada["unlinked"], "12.35");
    assert_eq!(ada["flags"], json!(["unlinked"]));
    assert_eq!(ada["payerCheck"]["payerGrossQuantity"], "12.345678912");
    assert_eq!(ada["payerCheck"]["matches"], true);
    assert_eq!(ada["groups"][0]["kind"], "trackable");
    assert_eq!(ada["groups"][0]["status"], "confirmed");
    let day = month["residualDays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|day| day["day"] == json!(today().to_string()))
        .unwrap();
    assert_eq!(day["residual"], "5.00");
    assert_eq!(day["status"], "provisional");
    assert_eq!(month["totals"]["unknownAccount"], "5.00");
    assert_eq!(month["orgCheck"]["payerGrossQuantity"], "17.345678912");
    assert_eq!(month["orgCheck"]["storedGrossQuantity"], "17.345678912");
    assert_eq!(month["orgCheck"]["matches"], true);
    assert_eq!(month["orgCheck"]["error"], Value::Null);
}

#[test]
fn account_errors_name_their_month_and_stale_reasons_are_structured() {
    let error = AccountErrorResponse::from(AiAccountImportError {
        account_id: crate::domain::models::AiProviderAccountId::new(1001).unwrap(),
        login: crate::domain::models::AiProviderLogin::parse("ada-lovelace").unwrap(),
        month: AiBillingMonth::parse("2026-09").unwrap(),
        error: "GitHub knows no Copilot usage for ada-lovelace".to_string(),
        seated: false,
        interrupted: false,
    });
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({
            "login": "ada-lovelace",
            "month": "2026-09",
            "error": "GitHub knows no Copilot usage for ada-lovelace",
            "seated": false,
            "interrupted": false
        })
    );
    let reason =
        |reason: AiStaleReason| serde_json::to_value(StaleReasonResponse::from(reason)).unwrap();
    assert_eq!(
        reason(AiStaleReason::LastSuccessOld {
            at: time::macros::datetime!(2026-10-25 07:00 UTC)
        }),
        json!({"kind": "lastSuccessOld", "at": "2026-10-25T07:00:00Z"})
    );
    assert_eq!(
        reason(AiStaleReason::SourceBehind {
            source: AiMeteredSource::AccountCredits,
            through: time::macros::date!(2026 - 10 - 20)
        }),
        json!({"kind": "sourceBehind", "source": "accountCredits", "through": "2026-10-20"})
    );
    assert_eq!(
        reason(AiStaleReason::NeverSucceeded),
        json!({"kind": "neverSucceeded"})
    );
}

#[test]
fn the_code_review_check_lists_each_days_difference() {
    use crate::domain::models::AiCodeReviewDay;
    let credits = |credits: i64| AiUsdNanos::new(credits * 10_000_000).unwrap();
    let check = AiCodeReviewCheck {
        by_accounts: credits(30),
        by_repositories: credits(20),
        days: vec![
            AiCodeReviewDay {
                day: time::macros::date!(2026 - 10 - 05),
                by_accounts: credits(20),
                by_repositories: credits(20),
                difference: 0,
                status: AiResidualStatus::Settled,
            },
            AiCodeReviewDay {
                day: time::macros::date!(2026 - 10 - 24),
                by_accounts: credits(10),
                by_repositories: AiUsdNanos::ZERO,
                difference: -10 * 10_000_000,
                status: AiResidualStatus::Provisional,
            },
        ],
    };
    assert_eq!(
        serde_json::to_value(CodeReviewResponse::from(check)).unwrap(),
        json!({
            "byAccounts": "30.00",
            "byRepositories": "20.00",
            "days": [
                {
                    "day": "2026-10-05",
                    "byAccounts": "20.00",
                    "byRepositories": "20.00",
                    "difference": "0.00",
                    "status": "settled",
                },
                {
                    "day": "2026-10-24",
                    "byAccounts": "10.00",
                    "byRepositories": "0.00",
                    "difference": "-10.00",
                    "status": "provisional",
                },
            ],
        })
    );
}

#[test]
fn credits_are_hundredths_rounded_half_up_with_signs_only_where_allowed() {
    assert_eq!(credits(AiUsdNanos::new(123_456_789).unwrap()), "12.35");
    assert_eq!(credits(AiUsdNanos::new(49_999).unwrap()), "0.00");
    assert_eq!(credits(AiUsdNanos::new(50_000).unwrap()), "0.01");
    assert_eq!(signed_credits(-50_000), "-0.01");
    assert_eq!(signed_credits(-49_999), "0.00");
    assert_eq!(signed_credits(-1_234_500_000), "-123.45");
}

#[test]
fn metered_usage_stays_off_the_agent_surface() {
    let document = serde_json::to_value(agent_openapi()).unwrap();
    let paths = document["paths"].as_object().unwrap();
    assert!(paths.keys().all(|path| !path.contains("metered-usage")));
}
