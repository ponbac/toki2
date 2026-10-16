use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    extract::Path,
    http::{
        header::{CONTENT_TYPE, COOKIE, SET_COOKIE},
        Method, Request, StatusCode,
    },
    middleware,
    routing::post,
};
use axum_login::{tower_sessions::SessionManagerLayer, AuthManagerLayerBuilder, AuthnBackend};
use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use serde_json::{json, Value};
use sqlx::PgPool;
use time::format_description::well_known::Rfc3339;
use tower::ServiceExt;
use tower_sessions_moka_store::MokaStore;

use super::*;
use crate::{
    adapters::outbound::postgres::PostgresBuddyRepository,
    auth::{require_authenticated, AuthBackend, AuthSession},
    domain::{
        models::{AiUsageTimeZone, DailyHours, UserId},
        ports::outbound::LoggedHours,
        services::BuddyServiceImpl,
    },
};

/// Test-only sign-in that starts a browser session for any stored user.
async fn sign_in(mut session: AuthSession, Path(user_id): Path<i64>) -> StatusCode {
    let user = session.backend.get_user(&user_id).await.unwrap().unwrap();
    session.login(&user).await.unwrap();
    StatusCode::NO_CONTENT
}

/// The same hours logged today, whenever they are read.
struct HoursToday(f64);

#[async_trait]
impl LoggedHours for HoursToday {
    async fn daily_worked_hours(
        &self,
        _: UserId,
        (_, today): (Date, Date),
    ) -> Result<Option<Vec<DailyHours>>, BuddyError> {
        Ok(Some(vec![DailyHours {
            date: today,
            worked_hours: self.0,
        }]))
    }
}

/// The routes behind a browser session, with `hours_today` logged today.
fn app(pool: &PgPool, hours_today: f64) -> Router {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    let service: Arc<dyn BuddyService> = Arc::new(BuddyServiceImpl::new(
        Arc::new(PostgresBuddyRepository::new(db.clone())),
        Arc::new(HoursToday(hours_today)),
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
    ));
    let client = BasicClient::new(ClientId::new("test-client".to_string()))
        .set_client_secret(ClientSecret::new("test-secret".to_string()))
        .set_auth_uri(AuthUrl::new("https://example.com/authorize".to_string()).unwrap())
        .set_token_uri(TokenUrl::new("https://example.com/token".to_string()).unwrap())
        .set_redirect_uri(RedirectUrl::new("https://example.com/callback".to_string()).unwrap());
    let sessions = SessionManagerLayer::new(MokaStore::new(Some(16))).with_secure(false);

    Router::new()
        .nest("/buddy", router())
        .route_layer(middleware::from_fn(require_authenticated))
        .route("/test-sign-in/{user_id}", post(sign_in))
        .with_state(service)
        .layer(AuthManagerLayerBuilder::new(AuthBackend::new(db, client), sessions).build())
}

/// A signed-in user's session cookie.
async fn test_user(app: &Router, pool: &PgPool, email: &str) -> String {
    signed_in_user(app, pool, email, "Test User").await.0
}

/// A signed-in user's session cookie and id.
async fn signed_in_user(
    app: &Router,
    pool: &PgPool,
    email: &str,
    full_name: &str,
) -> (String, i32) {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ($1, $2, '', '')
         RETURNING id",
    )
    .bind(email)
    .bind(full_name)
    .fetch_one(pool)
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/test-sign-in/{id}"))
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
    (cookie, id)
}

/// Sends a request to `/buddy` with a raw JSON body, if any.
async fn send(
    app: &Router,
    cookie: &str,
    method: Method,
    body: Option<&str>,
) -> (StatusCode, Value) {
    send_to(app, cookie, method, "/buddy", body).await
}

/// Sends a request to `uri` with a raw JSON body, if any.
async fn send_to(
    app: &Router,
    cookie: &str,
    method: Method,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(COOKIE, cookie);
    let request = match body {
        Some(body) => request
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    let response = app.clone().oneshot(request.unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get_profile(app: &Router, cookie: &str) -> (StatusCode, Value) {
    send(app, cookie, Method::GET, None).await
}

async fn put_profile(app: &Router, cookie: &str, body: Value) -> (StatusCode, Value) {
    send(app, cookie, Method::PUT, Some(&body.to_string())).await
}

async fn local_today(pool: &PgPool) -> String {
    let today: Date = sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Europe/Stockholm')::date")
        .fetch_one(pool)
        .await
        .unwrap();
    today.to_string()
}

/// Asserts a status with the JSON error body that the API documents.
fn assert_error((status, body): (StatusCode, Value), expected: StatusCode, case: &str) {
    assert_eq!(status, expected, "{case}: {body}");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()),
        "{case}: {body}"
    );
}

#[sqlx::test]
async fn the_first_update_enrols_and_opening_the_profile_counts_todays_hours(pool: PgPool) {
    let app = app(&pool, 6.5);
    let dev = test_user(&app, &pool, "dev@example.com").await;
    let today = local_today(&pool).await;

    assert_eq!(
        get_profile(&app, &dev).await,
        (StatusCode::OK, json!({ "profile": null }))
    );
    assert_error(
        put_profile(&app, &dev, json!({ "enabled": true })).await,
        StatusCode::BAD_REQUEST,
        "no lead",
    );

    let (status, enrolled) = put_profile(
        &app,
        &dev,
        json!({ "enabled": true, "leadSpeciesId": 25, "battleNotifications": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    assert_eq!(
        enrolled,
        json!({
            "profile": {
                "enabled": true,
                "battleNotifications": false,
                "buddies": [{
                    "slot": 1,
                    "speciesId": 25,
                    "visible": true,
                    "level": 1,
                    "xp": 0,
                    "levelXp": 0,
                    "nextLevelXp": 1,
                }],
                "partnerUnlockLevel": 10,
                "today": {
                    "date": today,
                    "loggedHours": 0.0,
                    "hoursXp": 0,
                    "battles": 0,
                    "battleXp": 0,
                },
                "hoursSyncedAt": null,
            }
        })
    );

    let (status, opened) = get_profile(&app, &dev).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let synced_at = opened["profile"]["hoursSyncedAt"].clone();
    assert!(
        synced_at
            .as_str()
            .is_some_and(|at| OffsetDateTime::parse(at, &Rfc3339).is_ok()),
        "{synced_at}"
    );
    assert_eq!(
        opened,
        json!({
            "profile": {
                "enabled": true,
                "battleNotifications": false,
                "buddies": [{
                    "slot": 1,
                    "speciesId": 25,
                    "visible": true,
                    "level": 9,
                    "xp": 390,
                    "levelXp": 359,
                    "nextLevelXp": 511,
                }],
                "partnerUnlockLevel": 10,
                "today": {
                    "date": today,
                    "loggedHours": 6.5,
                    "hoursXp": 390,
                    "battles": 0,
                    "battleXp": 0,
                },
                "hoursSyncedAt": synced_at,
            }
        })
    );
}

#[sqlx::test]
async fn the_partner_unlocks_at_lead_level_10(pool: PgPool) {
    // Nine hours give 540 XP, which is Lv 10.
    let app = app(&pool, 9.0);
    let dev = test_user(&app, &pool, "dev@example.com").await;
    let (status, _) =
        put_profile(&app, &dev, json!({ "enabled": true, "leadSpeciesId": 25 })).await;
    assert_eq!(status, StatusCode::OK);

    // Until the hours are read, the lead has no XP.
    let locked = put_profile(&app, &dev, json!({ "partnerSpeciesId": 1 })).await;
    assert_eq!(
        locked.1["error"],
        "the partner unlocks when your lead reaches Lv 10"
    );
    assert_error(locked, StatusCode::CONFLICT, "locked partner");
    assert_error(
        put_profile(&app, &dev, json!({ "partnerVisible": false })).await,
        StatusCode::CONFLICT,
        "no partner to hide",
    );

    let (status, _) = get_profile(&app, &dev).await;
    assert_eq!(status, StatusCode::OK);
    let (status, unlocked) = put_profile(&app, &dev, json!({ "partnerSpeciesId": 1 })).await;
    assert_eq!(status, StatusCode::OK, "{unlocked}");
    assert_eq!(
        unlocked["profile"]["buddies"],
        json!([
            {
                "slot": 1,
                "speciesId": 25,
                "visible": true,
                "level": 10,
                "xp": 540,
                "levelXp": 511,
                "nextLevelXp": 700,
            },
            {
                "slot": 2,
                "speciesId": 1,
                "visible": true,
                "level": 1,
                "xp": 0,
                "levelXp": 0,
                "nextLevelXp": 1,
            },
        ])
    );

    let (status, changed) = put_profile(
        &app,
        &dev,
        json!({ "partnerSpeciesId": 4, "partnerVisible": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        changed["profile"]["buddies"][1],
        json!({
            "slot": 2,
            "speciesId": 4,
            "visible": false,
            "level": 1,
            "xp": 0,
            "levelXp": 0,
            "nextLevelXp": 1,
        })
    );
}

#[sqlx::test]
async fn malformed_updates_and_unknown_species_are_bad_requests(pool: PgPool) {
    let app = app(&pool, 0.0);
    let dev = test_user(&app, &pool, "dev@example.com").await;

    for body in [
        // 514 is a national dex number without battle data or sprites.
        r#"{ "enabled": true, "leadSpeciesId": 514 }"#,
        r#"{ "enabled": true, "leadSpeciesId": 0 }"#,
        r#"{ "enabled": true, "leadSpeciesId": 70000 }"#,
        r#"{ "enabled": true, "leadSpeciesId": "25" }"#,
        r#"{ "enabled": true, "leadSpeciesId": 25.5 }"#,
        r#"{ "enabled": "yes", "leadSpeciesId": 25 }"#,
        r#"{ "enabled": true, "leadSpeciesId": 25, "energy": "calm" }"#,
        r#"{ "enabled": true, "#,
    ] {
        assert_error(
            send(&app, &dev, Method::PUT, Some(body)).await,
            StatusCode::BAD_REQUEST,
            body,
        );
    }
    assert_eq!(
        get_profile(&app, &dev).await,
        (StatusCode::OK, json!({ "profile": null }))
    );

    // Null leaves a setting as it is.
    let (status, enrolled) = put_profile(
        &app,
        &dev,
        json!({ "enabled": true, "leadSpeciesId": 25, "partnerSpeciesId": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    assert_error(
        put_profile(&app, &dev, json!({ "leadSpeciesId": 514 })).await,
        StatusCode::BAD_REQUEST,
        "unknown species after enrolment",
    );
    let (_, profile) = get_profile(&app, &dev).await;
    assert_eq!(profile["profile"]["buddies"][0]["speciesId"], 25);
}
