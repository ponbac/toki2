use std::{collections::HashMap, sync::Mutex};

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
use time::{macros::datetime, Duration};
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::outbound::postgres::{
        PostgresAiProviderAccountRepository, PostgresAiUsageRepository, PostgresApiTokenRepository,
    },
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{
            AiCoverageStatus, AiListedSeat, AiMachine, AiMachineId, AiPartialBucketCost, AiPricing,
            AiPricingStatus, AiProvider, AiProviderAccountProfile, AiProviderCoverage,
            AiProviderLogin, AiSeatListing, AiTokenCounts, AiUsageBucket, AiUsageUpload,
            AiUsageWindow,
        },
        ports::{
            inbound::{ApiTokenAuthenticator, ApiTokenService},
            outbound::{AiProviderAccountRepository, AiUsageRepository, ProviderAccountDirectory},
        },
        services::{AiAccountDirectoryState, AiProviderAccountServiceImpl, ApiTokenServiceImpl},
    },
};

const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

fn db(pool: &PgPool) -> crate::db::DbPool {
    crate::db::DbPool::from(pool.clone())
}

/// A stand-in for GitHub's seats and profiles, with synthetic people.
#[derive(Default)]
struct FakeDirectory {
    seats: Mutex<Vec<AiListedSeat>>,
    profiles: Mutex<HashMap<i64, (String, Option<String>)>>,
    failure: Mutex<Option<ProviderAccountDirectoryError>>,
}

impl FakeDirectory {
    /// A seat for `login` since `created_at`, with a public profile `name`.
    fn seat(&self, id: i64, login: &str, created_at: OffsetDateTime, name: Option<&str>) {
        self.seats.lock().unwrap().push(AiListedSeat {
            account_id: AiProviderAccountId::new(id).unwrap(),
            login: AiProviderLogin::parse(login).unwrap(),
            created_at,
            pending_cancellation_on: None,
            last_activity_at: Some(datetime!(2026-10-20 09:00 UTC)),
            last_activity_client: Some("vscode/1.143.0/copilot-chat/0.71.0".to_string()),
        });
        self.profiles
            .lock()
            .unwrap()
            .insert(id, (login.to_string(), name.map(str::to_string)));
    }

    fn fail(&self, error: ProviderAccountDirectoryError) {
        *self.failure.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl ProviderAccountDirectory for FakeDirectory {
    async fn seats(&self) -> Result<AiSeatListing, ProviderAccountDirectoryError> {
        if let Some(error) = self.failure.lock().unwrap().clone() {
            return Err(error);
        }
        let seats = self.seats.lock().unwrap().clone();
        Ok(AiSeatListing {
            reported_total: Some(u32::try_from(seats.len()).unwrap()),
            seats,
            ended_on_short_page: true,
        })
    }

    async fn profile(
        &self,
        account: AiProviderAccountId,
    ) -> Result<Option<AiProviderAccountProfile>, ProviderAccountDirectoryError> {
        Ok(self
            .profiles
            .lock()
            .unwrap()
            .get(&account.as_i64())
            .map(|(login, name)| AiProviderAccountProfile {
                account_id: account,
                login: AiProviderLogin::parse(login).unwrap(),
                name: name.clone(),
                email: None,
            }))
    }
}

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The routes composed as in production, over a ready directory.
fn app(pool: &PgPool, directory: Arc<FakeDirectory>) -> Router {
    app_with(pool, AiAccountDirectoryState::Ready(directory))
}

fn app_with(pool: &PgPool, directory: AiAccountDirectoryState) -> Router {
    let db = db(pool);
    let service: Arc<dyn AiProviderAccountService> = Arc::new(AiProviderAccountServiceImpl::new(
        Arc::new(PostgresAiProviderAccountRepository::new(db.clone())),
        directory,
        AiProvider::Copilot,
        "Example-AB".to_string(),
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

async fn admin(app: &Router, pool: &PgPool) -> TestUser {
    test_user(app, pool, "root@example.com", "Root Admin", &["Admin"]).await
}

/// Copilot usage of `user` yesterday, priced at `cost` dollars.
async fn copilot_usage(pool: &PgPool, user: UserId, cost: f64) {
    let now = OffsetDateTime::now_utc();
    let hour =
        now.replace_time(time::Time::from_hms(now.hour(), 0, 0).unwrap()) - Duration::days(1);
    let upload = AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(MACHINE).unwrap(),
            label: "laptop".to_string(),
            client_version: "0.10.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        AiUsageWindow::new(hour, hour + Duration::hours(1)).unwrap(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: None,
            source: "https://example.com/prices.json".to_string(),
        },
        vec![AiUsageBucket {
            billing_provider: None,
            hour_start: hour,
            session_key: "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f".to_string(),
            project_key: "github.com/example/app".to_string(),
            provider: AiProvider::Copilot,
            model: "gpt-6.1-sol".to_string(),
            tokens: AiTokenCounts {
                input: 100,
                cache_read: 1_000,
                cache_write: 10,
                output: 50,
            },
            records: 4,
            estimated_cost_usd: Some(cost),
            unpriced_records: 0,
        }],
        vec![AiProviderCoverage {
            provider: AiProvider::Copilot,
            status: AiCoverageStatus::Ok,
            files: 1,
            unreadable: 0,
            malformed_lines: 0,
            skipped_records: 0,
            duplicates: 0,
        }],
        Vec::new(),
        AiPartialBucketCost::PricedSubtotal,
    )
    .unwrap();
    PostgresAiUsageRepository::new(db(pool))
        .replace_window(&user, &upload)
        .await
        .unwrap();
}

fn account_json<'a>(overview: &'a Value, login: &str) -> &'a Value {
    overview["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|account| account["login"] == login)
        .unwrap_or_else(|| panic!("no account {login} in {overview}"))
}

#[sqlx::test]
async fn only_admins_in_a_browser_session_reach_provider_accounts(pool: PgPool) {
    let app = app(&pool, Arc::new(FakeDirectory::default()));
    let root = admin(&app, &pool).await;
    let dev = test_user(&app, &pool, "dev@example.com", "Dev Eloper", &["User"]).await;
    let uri = "/ai-usage/admin/provider-accounts";

    for (caller, expected) in [
        (Caller::Anonymous, StatusCode::UNAUTHORIZED),
        (Caller::Session(&dev.cookie), StatusCode::FORBIDDEN),
        (Caller::Token(&root.token), StatusCode::FORBIDDEN),
        (Caller::Session(&root.cookie), StatusCode::OK),
    ] {
        let (status, body) = send(&app, &caller, Method::GET, uri, None).await;
        assert_eq!(status, expected, "{body}");
    }
    let (status, _) = send(
        &app,
        &Caller::Token(&root.token),
        Method::POST,
        "/ai-usage/admin/provider-accounts/refresh",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn a_refresh_fills_the_overview_with_seats_and_suggestions(pool: PgPool) {
    let directory = Arc::new(FakeDirectory::default());
    let app = app(&pool, directory.clone());
    let root = admin(&app, &pool).await;
    let ada = test_user(&app, &pool, "ada@example.com", "Ada Lovelace", &["User"]).await;
    let alan = test_user(&app, &pool, "alan@example.com", "Alan Turing", &["User"]).await;
    copilot_usage(&pool, alan.id, 3.5).await;
    directory.seat(
        1001,
        "ada-gh",
        datetime!(2023-04-17 07:34:52 +02:00),
        Some("Ada Lovelace"),
    );
    directory.seat(1002, "aturing", datetime!(2025-09-16 10:00 +02:00), None);
    let session = Caller::Session(&root.cookie);

    let (status, before) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert_eq!(before["accounts"], json!([]));
    assert_eq!(before["directory"]["status"], "ready");
    assert_eq!(before["directory"]["lastSuccessAt"], Value::Null);

    let (status, refresh) = send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/provider-accounts/refresh",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refresh}");
    assert_eq!(refresh["seatsListed"], 2);
    assert_eq!(refresh["seatsReported"], 2);
    assert_eq!(refresh["complete"], true);
    assert_eq!(refresh["seatsStarted"], 2);
    assert_eq!(refresh["seatsEnded"], 0);
    assert_eq!(refresh["profilesRead"], 2);
    assert_eq!(refresh["profilesFailed"], 0);

    let (_, overview) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    assert_eq!(overview["provider"], "copilot");
    let directory_json = &overview["directory"];
    assert_eq!(directory_json["namespace"], "Example-AB");
    assert_eq!(directory_json["seatsListed"], 2);
    assert_eq!(directory_json["lastError"], Value::Null);
    assert!(directory_json["lastSuccessAt"].is_string());

    let ada_account = account_json(&overview, "ada-gh");
    assert_eq!(ada_account["accountId"], 1001);
    assert_eq!(ada_account["profile"]["name"], "Ada Lovelace");
    assert_eq!(ada_account["seats"][0]["firstDay"], "2023-04-17");
    assert_eq!(ada_account["seats"][0]["listed"], true);
    assert_eq!(ada_account["seats"][0]["lastDay"], Value::Null);
    assert_eq!(
        ada_account["seats"][0]["lastActivityClient"],
        "vscode/1.143.0/copilot-chat/0.71.0"
    );
    assert_eq!(ada_account["needsLook"], true);
    assert_eq!(ada_account["currentDispositionId"], Value::Null);
    assert_eq!(
        ada_account["defaultPeriod"],
        json!({"validFrom": "2023-04-17", "validTo": null})
    );
    assert_eq!(
        ada_account["suggestions"],
        json!([{"userId": ada.id.as_i32(), "confidence": "verified", "evidence": ["name"]}])
    );
    assert_eq!(
        ada_account["uncoveredSeatDays"][0]["from"],
        overview["reviewFrom"]
    );
    assert_eq!(
        ada_account["uncoveredSeatDays"][0]["through"],
        overview["today"]
    );

    let alan_account = account_json(&overview, "aturing");
    assert_eq!(
        alan_account["suggestions"],
        json!([{
            "userId": alan.id.as_i32(),
            "confidence": "probable",
            "evidence": ["loginInitialSurname"],
        }])
    );

    // Alan's Copilot usage has no link yet.
    assert_eq!(overview["activity"][0]["userId"], alan.id.as_i32());
    assert_eq!(overview["activity"][0]["days"], 1);
    assert_eq!(overview["activity"][0]["usage"]["apiEquivalentUsd"], 3.5);
    let unlinked = &overview["unlinkedUsers"][0];
    assert_eq!(unlinked["userId"], alan.id.as_i32());
    assert_eq!(unlinked["uncoveredDays"], 1);
    assert_eq!(unlinked["suggestedAccountIds"], json!([1002]));
    assert_eq!(unlinked["usage"]["records"], 4);
}

#[sqlx::test]
async fn a_disabled_directory_lists_known_accounts_but_cannot_refresh(pool: PgPool) {
    let app = app_with(&pool, AiAccountDirectoryState::Disabled);
    let root = admin(&app, &pool).await;
    let session = Caller::Session(&root.cookie);
    PostgresAiProviderAccountRepository::new(db(&pool))
        .store_seat_listing(
            AiProvider::Copilot,
            &AiSeatListing {
                seats: vec![AiListedSeat {
                    account_id: AiProviderAccountId::new(1001).unwrap(),
                    login: AiProviderLogin::parse("ada-gh").unwrap(),
                    created_at: datetime!(2026-01-01 08:00 UTC),
                    pending_cancellation_on: None,
                    last_activity_at: None,
                    last_activity_client: None,
                }],
                reported_total: Some(1),
                ended_on_short_page: true,
            },
        )
        .await
        .unwrap();

    let (status, overview) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["directory"]["status"], "disabled");
    assert_eq!(overview["directory"]["message"], Value::Null);
    assert_eq!(account_json(&overview, "ada-gh")["profile"], Value::Null);

    let (status, error) = send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/provider-accounts/refresh",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{error}");

    let misconfigured = app_with(
        &pool,
        AiAccountDirectoryState::Misconfigured(
            "github.app_private_key is not a PEM-encoded key".to_string(),
        ),
    );
    // Each app keeps its own sessions.
    let other_admin = test_user(
        &misconfigured,
        &pool,
        "other-root@example.com",
        "Other Admin",
        &["Admin"],
    )
    .await;
    let (status, overview) = send(
        &misconfigured,
        &Caller::Session(&other_admin.cookie),
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["directory"]["status"], "misconfigured");
    assert_eq!(
        overview["directory"]["message"],
        "github.app_private_key is not a PEM-encoded key"
    );
}

#[sqlx::test]
async fn refresh_failures_are_reported_and_recorded(pool: PgPool) {
    let directory = Arc::new(FakeDirectory::default());
    let app = app(&pool, directory.clone());
    let root = admin(&app, &pool).await;
    let session = Caller::Session(&root.cookie);
    let refresh = "/ai-usage/admin/provider-accounts/refresh";

    directory.fail(ProviderAccountDirectoryError::Forbidden(
        "the app needs GitHub Copilot Business: read".to_string(),
    ));
    let (status, error) = send(&app, &session, Method::POST, refresh, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("Copilot Business"),
        "{error}"
    );
    let (_, overview) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    assert!(overview["directory"]["lastError"]
        .as_str()
        .unwrap()
        .contains("Copilot Business"));
    assert!(overview["directory"]["lastAttemptAt"].is_string());
    assert_eq!(overview["directory"]["lastSuccessAt"], Value::Null);

    directory.fail(ProviderAccountDirectoryError::RateLimited { retry_after: None });
    let (status, _) = send(&app, &session, Method::POST, refresh, None).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn dispositions_are_created_ended_and_deleted(pool: PgPool) {
    let directory = Arc::new(FakeDirectory::default());
    let app = app(&pool, directory.clone());
    let root = admin(&app, &pool).await;
    let dev = test_user(&app, &pool, "dev@example.com", "Dev Eloper", &["User"]).await;
    let session = Caller::Session(&root.cookie);
    directory.seat(1001, "ada-gh", datetime!(2026-01-01 08:00 UTC), None);
    directory.seat(1002, "zz-top", datetime!(2026-01-01 08:00 UTC), None);
    send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/provider-accounts/refresh",
        None,
    )
    .await;
    let create = "/ai-usage/admin/provider-accounts/dispositions";

    let (status, created) = send(
        &app,
        &session,
        Method::POST,
        create,
        Some(json!({
            "accountId": 1001,
            "target": {"kind": "user", "userId": dev.id.as_i32()},
            "validFrom": "2026-01-01",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["accountId"], 1001);
    assert_eq!(created["login"], "ada-gh");
    assert_eq!(
        created["target"],
        json!({"kind": "user", "userId": dev.id.as_i32()})
    );
    assert_eq!(created["validFrom"], "2026-01-01");
    assert_eq!(created["validTo"], Value::Null);
    assert_eq!(created["updatedBy"], root.id.as_i32());
    assert!(created["updatedAt"].is_string());
    let id = created["id"].as_i64().unwrap();

    // Overlaps are conflicts that name the account.
    let (status, error) = send(
        &app,
        &session,
        Method::POST,
        create,
        Some(json!({"accountId": 1001, "target": {"kind": "external"}, "validFrom": "2026-06-01"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        error["error"].as_str().unwrap().contains("ada-gh"),
        "{error}"
    );
    let (status, error) = send(
        &app,
        &session,
        Method::POST,
        create,
        Some(json!({
            "accountId": 1002,
            "target": {"kind": "user", "userId": dev.id.as_i32()},
            "validFrom": "2026-06-01",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("already linked to ada-gh"),
        "{error}"
    );

    // End it, then hand the account over from the next day.
    let (status, ended) = send(
        &app,
        &session,
        Method::PUT,
        &format!("{create}/{id}"),
        Some(json!({
            "target": {"kind": "user", "userId": dev.id.as_i32()},
            "validFrom": "2026-01-01",
            "validTo": "2026-05-31",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ended}");
    assert_eq!(ended["validTo"], "2026-05-31");
    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        create,
        Some(json!({"accountId": 1001, "target": {"kind": "external"}, "validFrom": "2026-06-01"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (_, overview) = send(
        &app,
        &session,
        Method::GET,
        "/ai-usage/admin/provider-accounts",
        None,
    )
    .await;
    let account = account_json(&overview, "ada-gh");
    assert_eq!(account["dispositions"].as_array().unwrap().len(), 2);
    assert_eq!(account["needsLook"], false);
    assert_eq!(account["defaultPeriod"], Value::Null);
    assert_eq!(account["suggestions"], json!([]));

    // Refused bodies and missing things.
    for (body, expected) in [
        (
            json!({"accountId": 1002, "target": {"kind": "external"}, "validFrom": "2026-06-01", "validTo": "2026-05-01"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"accountId": 0, "target": {"kind": "external"}, "validFrom": "2026-06-01"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"accountId": 1002, "target": {"kind": "robot"}, "validFrom": "2026-06-01"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"accountId": 1002, "target": {"kind": "user"}, "validFrom": "2026-06-01"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"accountId": 1002, "target": {"kind": "external"}, "validFrom": "06/01/2026"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"accountId": 4040, "target": {"kind": "external"}, "validFrom": "2026-06-01"}),
            StatusCode::NOT_FOUND,
        ),
        (
            json!({"accountId": 1002, "target": {"kind": "user", "userId": 424242}, "validFrom": "2026-06-01"}),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, error) = send(&app, &session, Method::POST, create, Some(body.clone())).await;
        assert_eq!(status, expected, "{body}: {error}");
        assert!(error["error"].is_string(), "{error}");
    }
    let (status, _) = send(
        &app,
        &session,
        Method::PUT,
        &format!("{create}/424242"),
        Some(json!({"target": {"kind": "external"}, "validFrom": "2026-06-01"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &app,
        &session,
        Method::DELETE,
        &format!("{create}/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(
        &app,
        &session,
        Method::DELETE,
        &format!("{create}/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        &app,
        &session,
        Method::DELETE,
        &format!("{create}/not-a-number"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn accepting_suggestions_links_verified_matches_only(pool: PgPool) {
    let directory = Arc::new(FakeDirectory::default());
    let app = app(&pool, directory.clone());
    let root = admin(&app, &pool).await;
    let ada = test_user(&app, &pool, "ada@example.com", "Ada Lovelace", &["User"]).await;
    let alan = test_user(&app, &pool, "alan@example.com", "Alan Turing", &["User"]).await;
    let grace = test_user(&app, &pool, "grace@example.com", "Grace Hopper", &["User"]).await;
    let hedy = test_user(&app, &pool, "hedy@example.com", "Hedy Lamarr", &["User"]).await;
    let session = Caller::Session(&root.cookie);
    directory.seat(
        1001,
        "ada-gh",
        datetime!(2025-09-16 10:00 +02:00),
        Some("Ada Lovelace"),
    );
    directory.seat(1002, "aturing", datetime!(2025-09-16 10:00 +02:00), None);
    directory.seat(
        1003,
        "grace-gh",
        datetime!(2025-09-16 10:00 +02:00),
        Some("Grace Hopper"),
    );
    directory.seat(
        1004,
        "hedy-gh",
        datetime!(2025-09-16 10:00 +02:00),
        Some("Hedy Lamarr"),
    );
    send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/provider-accounts/refresh",
        None,
    )
    .await;
    // Hedy's account already has a disposition.
    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        "/ai-usage/admin/provider-accounts/dispositions",
        Some(json!({"accountId": 1004, "target": {"kind": "external"}, "validFrom": "2025-09-16", "validTo": "2025-12-31"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let accept = "/ai-usage/admin/provider-accounts/accept-suggestions";
    let (status, accepted) = send(
        &app,
        &session,
        Method::POST,
        accept,
        Some(json!({"picks": [
            {"accountId": 1001, "userId": ada.id.as_i32()},
            {"accountId": 1002, "userId": alan.id.as_i32()},
            {"accountId": 1003, "userId": ada.id.as_i32()},
            {"accountId": 1004, "userId": hedy.id.as_i32()},
            {"accountId": 1001, "userId": ada.id.as_i32()},
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let created = accepted["created"].as_array().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0]["accountId"], 1001);
    assert_eq!(
        created[0]["target"],
        json!({"kind": "user", "userId": ada.id.as_i32()})
    );
    // From the account's first seat day, ongoing.
    assert_eq!(created[0]["validFrom"], "2025-09-16");
    assert_eq!(created[0]["validTo"], Value::Null);
    assert_eq!(
        accepted["skipped"],
        json!([
            {"accountId": 1002, "reason": "notVerified"},
            {"accountId": 1003, "reason": "changed"},
            {"accountId": 1004, "reason": "alreadyDisposed"},
            {"accountId": 1001, "reason": "alreadyDisposed"},
        ])
    );

    // Grace's own pick goes through.
    let (_, accepted) = send(
        &app,
        &session,
        Method::POST,
        accept,
        Some(json!({"picks": [{"accountId": 1003, "userId": grace.id.as_i32()}]})),
    )
    .await;
    assert_eq!(accepted["created"].as_array().unwrap().len(), 1);

    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        accept,
        Some(json!({"picks": []})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &app,
        &session,
        Method::POST,
        accept,
        Some(json!({"picks": [{"accountId": -1, "userId": 1}]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
