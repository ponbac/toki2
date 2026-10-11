use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, SET_COOKIE},
        Method, Request,
    },
    routing::post,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use crate::{
    adapters::outbound::postgres::{PostgresAiSubscriptionRepository, PostgresApiTokenRepository},
    auth::{authenticate_bearer, require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::AiUsageTimeZone,
        ports::inbound::{ApiTokenAuthenticator, ApiTokenService},
        services::{AiSubscriptionServiceImpl, ApiTokenServiceImpl},
    },
};

use super::*;

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The routes composed as in production: bearer tokens and browser sessions
/// both authenticate, and a bearer token takes precedence over a cookie.
fn app(pool: &PgPool) -> Router {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let service: Arc<dyn AiSubscriptionService> = Arc::new(AiSubscriptionServiceImpl::new(
        Arc::new(PostgresAiSubscriptionRepository::new(db.clone())),
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
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

/// How a request authenticates.
enum Caller<'a> {
    Token(&'a str),
    Session(&'a str),
    /// An API token and a session cookie together.
    TokenAndSession(&'a str, &'a str),
}

/// A request body: JSON, or raw text with a content type, if any.
enum Payload<'a> {
    None,
    Json(Value),
    Raw(Option<&'a str>, &'a str),
}

async fn send(
    app: &Router,
    caller: &Caller<'_>,
    method: Method,
    uri: &str,
    payload: Payload<'_>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    request = match caller {
        Caller::Token(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
        Caller::Session(cookie) => request.header(COOKIE, *cookie),
        Caller::TokenAndSession(token, cookie) => request
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(COOKIE, *cookie),
    };
    let body = match payload {
        Payload::None => Body::empty(),
        Payload::Json(body) => {
            request = request.header(CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        Payload::Raw(content_type, body) => {
            if let Some(content_type) = content_type {
                request = request.header(CONTENT_TYPE, content_type);
            }
            Body::from(body.to_string())
        }
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn call(
    app: &Router,
    caller: &Caller<'_>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let payload = body.map_or(Payload::None, Payload::Json);
    send(app, caller, method, uri, payload).await
}

struct TestUser {
    id: i32,
    token: String,
    cookie: String,
}

async fn test_user(app: &Router, pool: &PgPool, email: &str, roles: &[&str]) -> TestUser {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ($1, 'Test User', '', '', $2)
         RETURNING id",
    )
    .bind(email)
    .bind(roles)
    .fetch_one(pool)
    .await
    .unwrap();
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let token = ApiTokenServiceImpl::new(Arc::new(PostgresApiTokenRepository::new(db)))
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

fn claude_max(valid_from: &str, valid_to: Option<&str>) -> Value {
    json!({
        "provider": "claude",
        "plan": "Claude Max 5x",
        "monthlyCost": "1100",
        "currency": "sek",
        "validFrom": valid_from,
        "validTo": valid_to,
    })
}

/// Asserts a `400` with the JSON error body that the API documents.
fn assert_bad_request((status, body): (StatusCode, Value), case: &str) {
    assert_eq!(status, StatusCode::BAD_REQUEST, "{case}: {body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()),
        "{case}: {body}"
    );
}

#[sqlx::test]
async fn developers_reach_only_their_own_subscriptions(pool: PgPool) {
    let app = app(&pool);
    let owner = test_user(&app, &pool, "owner@example.com", &["User"]).await;
    let other = test_user(&app, &pool, "other@example.com", &["User"]).await;
    let owner_token = Caller::Token(&owner.token);
    let other_token = Caller::Token(&other.token);

    let (status, created) = call(
        &app,
        &owner_token,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-09-01", None)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        created,
        json!({
            "id": created["id"],
            "userId": owner.id,
            "provider": "claude",
            "plan": "Claude Max 5x",
            "monthlyCost": "1100.00",
            "currency": "SEK",
            "validFrom": "2026-09-01",
            "validTo": null,
        })
    );
    let path = format!("/ai-usage/subscriptions/{}", created["id"]);

    let (status, listed) = call(
        &app,
        &other_token,
        Method::GET,
        "/ai-usage/subscriptions",
        None,
    )
    .await;
    assert_eq!(
        (status, &listed["subscriptions"]),
        (StatusCode::OK, &json!([]))
    );
    for (method, body) in [
        (Method::PUT, Some(claude_max("2026-10-01", None))),
        (Method::DELETE, None),
    ] {
        let (status, _) = call(&app, &other_token, method, &path, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // Another developer can declare the same provider and period.
    let (status, _) = call(
        &app,
        &other_token,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-09-01", None)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, updated) = call(
        &app,
        &owner_token,
        Method::PUT,
        &path,
        Some(claude_max("2026-09-01", Some("2026-09-30"))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["validTo"], "2026-09-30");
    let (_, listed) = call(
        &app,
        &owner_token,
        Method::GET,
        "/ai-usage/subscriptions",
        None,
    )
    .await;
    assert_eq!(listed["subscriptions"], json!([updated]));

    let (status, _) = call(&app, &owner_token, Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn lists_name_the_time_zone_and_its_today(pool: PgPool) {
    let app = app(&pool);
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let today: Date = sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Stockholm')::date")
        .fetch_one(&pool)
        .await
        .unwrap();

    let (status, listed) = call(
        &app,
        &Caller::Token(&dev.token),
        Method::GET,
        "/ai-usage/subscriptions",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        listed,
        json!({
            "timeZone": "Europe/Stockholm",
            "today": today.to_string(),
            "subscriptions": [],
        })
    );
}

#[sqlx::test]
async fn invalid_or_overlapping_terms_are_rejected(pool: PgPool) {
    let app = app(&pool);
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let dev = Caller::Session(&dev.cookie);
    let (status, _) = call(
        &app,
        &dev,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-01-01", None)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = call(
        &app,
        &dev,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-12-01", Some("2026-12-31"))),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"],
        "the period overlaps another claude subscription of the same user"
    );

    for (field, value, expected) in [
        ("monthlyCost", json!("11.999"), "monthlyCost"),
        ("currency", json!("kronor"), "currency"),
        ("plan", json!(" "), "plan"),
        ("validTo", json!("2025-12-31"), "validTo"),
    ] {
        let mut terms = claude_max("2026-01-01", None);
        terms["provider"] = json!("codex");
        terms[field] = value;
        let (status, body) = call(
            &app,
            &dev,
            Method::POST,
            "/ai-usage/subscriptions",
            Some(terms),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}");
        assert!(
            body["error"].as_str().unwrap().contains(expected),
            "{field}: {body}"
        );
    }
}

#[sqlx::test]
async fn malformed_requests_are_json_bad_requests(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(&app, &pool, "admin@example.com", &["User", "Admin"]).await;
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let dev_token = Caller::Token(&dev.token);
    let admin_session = Caller::Session(&admin.cookie);
    let (status, created) = call(
        &app,
        &dev_token,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-01-01", Some("2026-01-31"))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let own = format!("/ai-usage/subscriptions/{}", created["id"]);
    let any = format!("/ai-usage/admin/subscriptions/{}", created["id"]);

    let untyped = claude_max("2026-01-01", None).to_string();
    let mut malformed = Vec::new();
    for (field, value) in [
        ("validFrom", json!("2026-13-01")),
        ("validFrom", json!("1 January 2026")),
        ("validFrom", json!("-0001-01-01")),
        ("validFrom", json!("0000-01-01")),
        ("validFrom", json!("10000-01-01")),
        ("validFrom", json!("+2026-01-01")),
        ("validTo", json!("0000-01-01")),
        ("validTo", json!("-0001-01-01")),
        ("validTo", json!("10000-01-01")),
        ("validTo", json!("+2026-01-31")),
        ("validTo", json!(20260131)),
        ("provider", json!("openai")),
        ("monthlyCost", json!(1100)),
        ("plan", Value::Null),
    ] {
        let mut body = claude_max("2026-01-01", None);
        body[field] = value.clone();
        malformed.push((format!("{field} = {value}"), Payload::Json(body)));
    }
    let mut missing = claude_max("2026-01-01", None);
    missing.as_object_mut().unwrap().remove("currency");
    malformed.push(("no currency".to_string(), Payload::Json(missing)));
    malformed.push((
        "not JSON".to_string(),
        Payload::Raw(Some("application/json"), "{"),
    ));
    malformed.push(("no content type".to_string(), Payload::Raw(None, &untyped)));
    malformed.push((
        "form content".to_string(),
        Payload::Raw(Some("application/x-www-form-urlencoded"), "plan=Pro"),
    ));

    for (case, payload) in malformed {
        let (caller, method, uri, payload) = match &payload {
            Payload::Json(body) => {
                let mut admin_body = body.clone();
                admin_body["userId"] = json!(dev.id);
                for (caller, method, uri, body) in [
                    (&dev_token, Method::PUT, own.as_str(), body.clone()),
                    (
                        &admin_session,
                        Method::POST,
                        "/ai-usage/admin/subscriptions",
                        admin_body,
                    ),
                    (&admin_session, Method::PUT, any.as_str(), body.clone()),
                ] {
                    assert_bad_request(
                        send(&app, caller, method.clone(), uri, Payload::Json(body)).await,
                        &format!("{method} {uri}: {case}"),
                    );
                }
                (&dev_token, Method::POST, "/ai-usage/subscriptions", payload)
            }
            _ => (&dev_token, Method::POST, "/ai-usage/subscriptions", payload),
        };
        assert_bad_request(send(&app, caller, method, uri, payload).await, &case);
    }

    for (method, uri) in [
        (Method::DELETE, "/ai-usage/subscriptions/first"),
        (
            Method::GET,
            "/ai-usage/subscription-mismatches?from=yesterday",
        ),
        (
            Method::GET,
            "/ai-usage/subscription-mismatches?from=-0001-01-01&to=2026-09-30",
        ),
        (
            Method::GET,
            "/ai-usage/subscription-mismatches?from=0000-01-01&to=2026-09-30",
        ),
        (
            Method::GET,
            "/ai-usage/subscription-mismatches?from=2026-01-01&to=10000-01-01",
        ),
        (
            Method::GET,
            "/ai-usage/subscription-mismatches?from=2026-09-02&to=2026-09-01",
        ),
    ] {
        assert_bad_request(call(&app, &dev_token, method, uri, None).await, uri);
    }
    assert_bad_request(
        call(
            &app,
            &admin_session,
            Method::GET,
            "/ai-usage/admin/subscriptions?userId=everyone",
            None,
        )
        .await,
        "admin userId",
    );

    // The largest positive four-digit date still has a valid wire projection.
    let (status, created) = call(
        &app,
        &dev_token,
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("9999-12-31", Some("9999-12-31"))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["validFrom"], "9999-12-31");
    assert_eq!(created["validTo"], "9999-12-31");
}

#[sqlx::test]
async fn mismatches_are_searched_between_local_days(pool: PgPool) {
    let app = app(&pool);
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let dev_token = Caller::Token(&dev.token);

    let (status, body) = call(
        &app,
        &dev_token,
        Method::GET,
        "/ai-usage/subscription-mismatches?from=2026-09-01&to=2026-09-30",
        None,
    )
    .await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "from": "2026-09-01", "to": "2026-09-30", "mismatches": [] })
        )
    );

    let (status, body) = call(
        &app,
        &dev_token,
        Method::GET,
        "/ai-usage/subscription-mismatches?to=2026-09-30",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (&body["from"], &body["to"]),
        (&json!("2026-07-03"), &json!("2026-09-30"))
    );
}

#[sqlx::test]
async fn admins_reach_everyones_subscriptions_through_a_session(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(&app, &pool, "admin@example.com", &["User", "Admin"]).await;
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let admin_session = Caller::Session(&admin.cookie);

    let mut for_dev = claude_max("2026-09-01", None);
    for_dev["userId"] = json!(dev.id);
    let (status, created) = call(
        &app,
        &admin_session,
        Method::POST,
        "/ai-usage/admin/subscriptions",
        Some(for_dev),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["userId"], dev.id);
    let (_, own) = call(
        &app,
        &Caller::Token(&dev.token),
        Method::GET,
        "/ai-usage/subscriptions",
        None,
    )
    .await;
    assert_eq!(own["subscriptions"], json!([created]));

    let (status, _) = call(
        &app,
        &Caller::Token(&admin.token),
        Method::POST,
        "/ai-usage/subscriptions",
        Some(claude_max("2026-09-01", None)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (_, everyone) = call(
        &app,
        &admin_session,
        Method::GET,
        "/ai-usage/admin/subscriptions",
        None,
    )
    .await;
    assert_eq!(everyone["subscriptions"].as_array().unwrap().len(), 2);
    assert_eq!(everyone["timeZone"], "Europe/Stockholm");
    let (_, filtered) = call(
        &app,
        &admin_session,
        Method::GET,
        &format!("/ai-usage/admin/subscriptions?userId={}", dev.id),
        None,
    )
    .await;
    assert_eq!(filtered["subscriptions"], json!([created]));

    let path = format!("/ai-usage/admin/subscriptions/{}", created["id"]);
    let (status, updated) = call(
        &app,
        &admin_session,
        Method::PUT,
        &path,
        Some(claude_max("2026-09-01", Some("2026-09-30"))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (&updated["userId"], &updated["validTo"]),
        (&json!(dev.id), &json!("2026-09-30"))
    );
    let (status, mismatches) = call(
        &app,
        &admin_session,
        Method::GET,
        &format!(
            "/ai-usage/admin/subscription-mismatches?userId={}&from=2026-09-01&to=2026-09-30",
            dev.id
        ),
        None,
    )
    .await;
    assert_eq!(
        (status, mismatches),
        (
            StatusCode::OK,
            json!({ "from": "2026-09-01", "to": "2026-09-30", "mismatches": [] })
        )
    );
    let (status, _) = call(&app, &admin_session, Method::DELETE, &path, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test]
async fn admin_routes_need_a_plain_admin_session(pool: PgPool) {
    let app = app(&pool);
    let admin = test_user(&app, &pool, "admin@example.com", &["User", "Admin"]).await;
    let dev = test_user(&app, &pool, "dev@example.com", &["User"]).await;
    let admin_routes = [
        (Method::GET, "/ai-usage/admin/subscriptions"),
        (Method::GET, "/ai-usage/admin/subscription-mismatches"),
        (Method::DELETE, "/ai-usage/admin/subscriptions/1"),
    ];

    for caller in [
        Caller::Session(&dev.cookie),
        Caller::Token(&dev.token),
        // API tokens never carry admin rights, as on every admin route.
        Caller::Token(&admin.token),
        // A developer's token must not borrow an admin's session cookie.
        Caller::TokenAndSession(&dev.token, &admin.cookie),
        Caller::TokenAndSession(&admin.token, &admin.cookie),
    ] {
        for (method, uri) in admin_routes.clone() {
            let (status, body) = call(&app, &caller, method, uri, None).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
            if matches!(caller, Caller::TokenAndSession(..)) {
                assert_eq!(
                    body["error"],
                    "only an admin signed in to Toki, not using an API token, can use admin routes"
                );
            }
        }
    }

    let (status, _) = call(
        &app,
        &Caller::Session(&admin.cookie),
        Method::GET,
        "/ai-usage/admin/subscriptions",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let request = Request::builder()
        .uri("/ai-usage/admin/subscriptions")
        .body(Body::empty())
        .unwrap();
    let anonymous = app.oneshot(request).await.unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
}
