use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        Method, Request, StatusCode,
    },
    Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

use super::super::tests::{
    app, payload, postgres_service, put, AnyTokenAuthenticator, RecordingService, MACHINE,
};
use crate::{
    adapters::outbound::postgres::PostgresApiTokenRepository,
    domain::{models::UserId, ports::inbound::ApiTokenService, services::ApiTokenServiceImpl},
};

const BASIS: &str = "copilot-rates@2026-10-24;litellm,tiers";

/// A bucket of `payload`'s shape at `hour` (`HH:00`, UTC on 2026-09-22).
fn bucket(hour: &str, session: u8, cost: f64) -> Value {
    json!({
        "hourStart": format!("2026-09-22T{hour}:00:00.000Z"),
        "sessionKey": format!("{session:032x}"),
        "project": "github.com/example/app",
        "provider": "claude",
        "model": "example-model",
        "tokens": { "input": 1200, "cacheRead": 48000, "cacheWrite": 3000, "output": 900 },
        "records": 14,
        "estimatedCostUsd": cost,
        "unpricedRecords": 0,
    })
}

fn repricing(buckets: Vec<Value>) -> Value {
    json!({
        "version": 1,
        "clientVersion": "0.8.0",
        "pricing": {
            "status": "fresh",
            "fetchedAt": "2026-10-24T10:00:00Z",
            "source": "https://example.com/prices.json",
            "basis": BASIS
        },
        "buckets": buckets
    })
}

async fn send(
    app: Router,
    method: Method,
    uri: &str,
    token: &str,
    body: Option<&Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn discover(app: Router, token: &str, query: &str) -> (StatusCode, Value) {
    let uri = format!("/ai-usage/machines/{MACHINE}/repricing{query}");
    send(app, Method::GET, &uri, token, None).await
}

async fn reprice(app: Router, token: &str, body: &Value) -> (StatusCode, Value) {
    let uri = format!("/ai-usage/machines/{MACHINE}/repricing");
    send(app, Method::POST, &uri, token, Some(body)).await
}

#[tokio::test]
async fn discovery_needs_a_basis_of_1_to_128_characters() {
    let app = app(
        Arc::new(RecordingService::default()),
        Arc::new(AnyTokenAuthenticator),
    );

    for query in ["", "?basis=", &format!("?basis={}", "a".repeat(129))] {
        let (status, body) = discover(app.clone(), "toki_any", query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert!(body["error"].is_string(), "{query}: {body}");
    }
    let (status, body) = send(
        app.clone(),
        Method::GET,
        "/ai-usage/machines/not-a-uuid/repricing?basis=new",
        "toki_any",
        None,
    )
    .await;
    assert_eq!(
        (status, body),
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": "machine_id must be a UUID" })
        )
    );

    let query = format!("?basis={}", "a".repeat(128));
    let (status, body) = discover(app, "toki_any", &query).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "floor": "2026-09-22T09:00:00Z", "providers": [] })
        )
    );
}

#[tokio::test]
async fn a_repricing_must_be_a_valid_version_1_payload() {
    let service = Arc::new(RecordingService::default());
    let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));
    let valid = repricing(vec![bucket("08", 1, 0.5), bucket("09", 1, 0.5)]);

    let mut version_2 = valid.clone();
    version_2["version"] = json!(2);
    let mut without_basis = valid.clone();
    without_basis["pricing"]
        .as_object_mut()
        .unwrap()
        .remove("basis");
    let mut empty_basis = valid.clone();
    empty_basis["pricing"]["basis"] = json!("");
    let mut long_basis = valid.clone();
    long_basis["pricing"]["basis"] = json!("a".repeat(129));
    let repeated = repricing(vec![bucket("09", 1, 0.5), bucket("09", 1, 0.7)]);
    for (case, body, error) in [
        (
            "version 2",
            &version_2,
            "unsupported repricing payload version 2; only version 1 is supported",
        ),
        (
            "empty basis",
            &empty_basis,
            "pricing.basis must be 1 to 128 characters without NUL",
        ),
        (
            "long basis",
            &long_basis,
            "pricing.basis must be 1 to 128 characters without NUL",
        ),
        (
            "repeated key",
            &repeated,
            "buckets[1]: duplicate key (hourStart, sessionKey, project, provider, model, billingProvider)",
        ),
    ] {
        let (status, body) = reprice(app.clone(), "toki_any", body).await;
        assert_eq!(
            (status, &body["error"]),
            (StatusCode::BAD_REQUEST, &json!(error)),
            "{case}"
        );
    }
    let (status, body) = reprice(app.clone(), "toki_any", &without_basis).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|error| error.contains("missing field `basis`")),
        "{body}"
    );
    assert!(service.reprices.lock().unwrap().is_empty());

    let (status, body) = reprice(app, "toki_any", &valid).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "updated": 7, "unmatched": 3, "beforeFloor": 2 })
        )
    );
    let reprices = service.reprices.lock().unwrap();
    assert_eq!(reprices[0].basis().as_str(), BASIS);
}

#[sqlx::test]
async fn stored_usage_is_repriced_from_the_floor_on(pool: PgPool) {
    let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
    )));
    let app = app(postgres_service(&pool), tokens.clone());
    let mut secrets = Vec::new();
    for email in ["owner@example.com", "other@example.com"] {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ($1, 'Test User', '', '')
             RETURNING id",
        )
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();
        let issued = tokens
            .create(&UserId::new(id), "token-ledger")
            .await
            .unwrap();
        secrets.push(issued.secret.as_str().to_string());
    }
    let (owner, other) = (&secrets[0], &secrets[1]);

    // Uploaded without a basis: one hour before the floor, two after it, and
    // Copilot next edit suggestions, which have no API price.
    let mut suggestions = bucket("10", 4, 0.0);
    suggestions["provider"] = json!("copilot");
    suggestions["model"] = json!("copilot-nes-oct");
    suggestions["estimatedCostUsd"] = Value::Null;
    suggestions["unpricedRecords"] = json!(14);
    let mut upload = payload(0);
    upload["version"] = json!(3);
    upload["buckets"] = json!([
        bucket("08", 1, 0.1),
        bucket("09", 2, 0.1),
        bucket("10", 3, 0.1),
        suggestions.clone(),
    ]);
    upload["coverage"][1]["provider"] = json!("copilot");
    upload["providerHints"] = json!([]);
    let (status, _) = put(app.clone(), MACHINE, Some(owner), &upload).await;
    assert_eq!(status, StatusCode::OK);

    let query = format!("?basis={BASIS}");
    let (status, body) = discover(app.clone(), owner, &query).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({
                "floor": "2026-09-22T09:00:00Z",
                "providers": [
                    { "provider": "claude", "from": "2026-09-22T09:00:00Z", "staleBuckets": 2 },
                    { "provider": "copilot", "from": "2026-09-22T10:00:00Z", "staleBuckets": 1 }
                ]
            })
        )
    );

    let new_costs = repricing(vec![
        bucket("08", 1, 0.5),
        bucket("09", 2, 0.5),
        bucket("10", 3, 0.5),
        suggestions,
    ]);
    let (status, body) = reprice(app.clone(), other, &new_costs).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "updated": 0, "unmatched": 3, "beforeFloor": 1 })
        )
    );
    let (status, body) = discover(app.clone(), other, &query).await;
    assert_eq!((status, &body["providers"]), (StatusCode::OK, &json!([])));
    let machine_basis = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT pricing_basis FROM ai_usage_machines WHERE id = $1::uuid",
        )
        .bind(MACHINE)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(machine_basis().await, None);

    // A repricing with nothing from the floor on still records the basis the
    // machine prices with now.
    let (status, body) = reprice(app.clone(), owner, &repricing(vec![bucket("08", 1, 0.5)])).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "updated": 0, "unmatched": 0, "beforeFloor": 1 })
        )
    );
    assert_eq!(machine_basis().await.as_deref(), Some(BASIS));

    let (status, body) = reprice(app.clone(), owner, &new_costs).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "updated": 3, "unmatched": 0, "beforeFloor": 1 })
        )
    );
    let stored: Vec<(String, Option<f64>, i64, Option<String>)> = sqlx::query_as(
        "SELECT to_char(hour_start AT TIME ZONE 'UTC', 'HH24'), estimated_cost_usd::float8,
             unpriced_records, pricing_basis
         FROM ai_usage_buckets
         ORDER BY hour_start, provider",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let new = |hour: &str, cost| (hour.to_string(), Some(cost), 0, Some(BASIS.to_string()));
    assert_eq!(
        stored,
        [
            ("08".to_string(), Some(0.1), 0, None),
            new("09", 0.5),
            new("10", 0.5),
            new("10", 0.0),
        ]
    );
    let (status, body) = discover(app, owner, &query).await;
    assert_eq!(
        (status, &body["providers"]),
        (
            StatusCode::OK,
            &json!([
                { "provider": "claude", "from": null, "staleBuckets": 0 },
                { "provider": "copilot", "from": null, "staleBuckets": 0 }
            ])
        )
    );
}

#[sqlx::test]
async fn closed_months_before_the_first_open_month_are_never_repriced(pool: PgPool) {
    let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
        PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
    )));
    let app = app(postgres_service(&pool), tokens.clone());
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ('owner@example.com', 'Test User', '', '')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let owner = tokens
        .create(&UserId::new(id), "token-ledger")
        .await
        .unwrap()
        .secret
        .as_str()
        .to_string();
    // September's two buckets, and one in October's first local hour
    // (00:00 in Stockholm, 30 September 22:00 UTC).
    let october_first_hour = |cost: f64| {
        let mut bucket = bucket("09", 4, cost);
        bucket["hourStart"] = json!("2026-09-30T22:00:00.000Z");
        bucket
    };
    let mut upload = payload(0);
    upload["version"] = json!(3);
    upload["window"]["end"] = json!("2026-10-01T22:00:00.000Z");
    upload["buckets"] = json!([
        bucket("09", 2, 0.1),
        bucket("10", 3, 0.1),
        october_first_hour(0.1)
    ]);
    upload["providerHints"] = json!([]);
    let (status, _) = put(app.clone(), MACHINE, Some(&owner), &upload).await;
    assert_eq!(status, StatusCode::OK);

    // September's AI bill closed: its costs are frozen, and the floor moves
    // to when October starts, the earlier of its UTC and Stockholm midnight:
    // October's local days stay repriceable from their first hour on.
    sqlx::query(
        "INSERT INTO ai_billing_month_closes
             (month, readiness_json, bill_json, developer_json, csv_header, projection_version)
         VALUES ('2026-09-01', '{}', '{}', '{}', ARRAY['month'], 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = discover(app.clone(), &owner, &format!("?basis={BASIS}")).await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({
                "floor": "2026-09-30T22:00:00Z",
                "providers": [{
                    "provider": "claude",
                    "from": "2026-09-30T22:00:00Z",
                    "staleBuckets": 1
                }]
            })
        )
    );
    let (status, body) = reprice(
        app.clone(),
        &owner,
        &repricing(vec![
            bucket("09", 2, 0.5),
            bucket("10", 3, 0.5),
            october_first_hour(0.5),
        ]),
    )
    .await;
    assert_eq!(
        (status, body),
        (
            StatusCode::OK,
            json!({ "updated": 1, "unmatched": 0, "beforeFloor": 2 })
        )
    );
    let costs: Vec<f64> = sqlx::query_scalar(
        "SELECT estimated_cost_usd::float8 FROM ai_usage_buckets ORDER BY hour_start",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(costs, [0.1, 0.1, 0.5]);

    // Re-opened, the configured floor applies again.
    sqlx::query("UPDATE ai_billing_month_closes SET reopened_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = discover(app.clone(), &owner, &format!("?basis={BASIS}")).await;
    assert_eq!(body["floor"], "2026-09-22T09:00:00Z");

    // October closes while September is open: September is the first open
    // month, so both stay repriceable, and a reprice of October posts on
    // November as an adjustment.
    sqlx::query(
        "INSERT INTO ai_billing_month_closes
             (month, readiness_json, bill_json, developer_json, csv_header, projection_version)
         VALUES ('2026-10-01', '{}', '{}', '{}', ARRAY['month'], 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (_, body) = discover(app.clone(), &owner, &format!("?basis={BASIS}")).await;
    assert_eq!(body["floor"], "2026-09-22T09:00:00Z");
}
