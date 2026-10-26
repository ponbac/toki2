//! Month close over HTTP: June to September 2026, with final monthly USD
//! rates, a developer's API usage in June billed to Client A, and the
//! changes after each close posting exactly once on the next open month.

use std::collections::{BTreeMap, BTreeSet};

use super::copilot::{copilot_august, upload_ada_copilot_only, CopilotAugust};
use super::*;
use crate::{
    adapters::outbound::postgres::PostgresAiRepositoryMappingRepository,
    domain::{
        models::{
            following, AiAccountDispositionTerms, AiAccountPeriod, AiAccountTarget, AiImportKind,
            AiImportPlan, AiMonthMode, AiPricingBasis, AiProviderAccountId, AiUsdNanos,
        },
        ports::{
            inbound::{AiMeteredUsageService, AiUsageService},
            outbound::{
                AiMeteredUsageRepository, AiProviderAccountRepository,
                AiRepositoryMappingRepository,
            },
        },
        services::{
            AiMeteredImportSettings, AiMeteredSourceState, AiMeteredUsageServiceImpl,
            AiUsageServiceImpl,
        },
    },
};

const CLIENT_B: (&str, &str) = ("102", "Client B");
const CLIENT_C: (&str, &str) = ("103", "Client C");

/// Final averages for June through September: 10.50, 10.25, 10.50, 10.00.
fn rates() -> Arc<FakeRates> {
    Arc::new(
        FakeRates::default()
            .monthly("2026-06", "USD", "10.5")
            .monthly("2026-07", "USD", "10.25")
            .monthly("2026-08", "USD", "10.5")
            .monthly("2026-09", "USD", "10"),
    )
}

fn close_app(pool: &PgPool, rates: Arc<FakeRates>) -> Router {
    app_with(
        pool,
        rates,
        AiExchangeRateSettings::default(),
        None,
        REPRICE_FLOOR,
    )
}

struct June {
    admin: TestUser,
    dev: TestUser,
}

/// An admin, and a developer whose laptop uploaded June to September (local
/// days, through `until`) with 12.50 USD of Claude in June on APP, mapped to
/// Client A.
async fn june(app: &Router, pool: &PgPool, until: OffsetDateTime) -> June {
    let admin = test_user(
        app,
        pool,
        "admin@example.com",
        "Admin Example",
        &["User", "Admin"],
    )
    .await;
    let dev = test_user(app, pool, "ada@example.com", "Ada Lovelace", &["User"]).await;
    map(pool, APP, "101", "Client A", admin.id).await;
    upload(
        pool,
        dev.id,
        LAPTOP,
        "work-laptop",
        (datetime!(2026-05-31 22:00 UTC), until),
        vec![bucket(
            datetime!(2026-06-10 08:00 UTC),
            0,
            APP,
            AiProvider::Claude,
            5,
            Some(12.5),
        )],
    )
    .await;
    June { admin, dev }
}

/// Stockholm's 1 October: the uploads cover June through September.
const THROUGH_SEPTEMBER: OffsetDateTime = datetime!(2026-09-30 22:00 UTC);

async fn post_close(
    app: &Router,
    caller: &Caller<'_>,
    month: &str,
    waivers: Value,
) -> (StatusCode, Value) {
    let (status, _, body) = send(
        app,
        caller,
        Method::POST,
        &format!("/ai-usage/admin/billing/{month}/close"),
        Some(json!({ "waivers": waivers })),
    )
    .await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

/// Closes `month` with no waivers, which must succeed: its close id.
async fn close(app: &Router, caller: &Caller<'_>, month: &str) -> i64 {
    let (status, body) = post_close(app, caller, month, json!([])).await;
    assert_eq!(status, StatusCode::CREATED, "{month}: {body}");
    assert_eq!(body["state"], "closed");
    body["close"]["closeId"].as_i64().unwrap()
}

async fn reopen(app: &Router, caller: &Caller<'_>, month: &str) -> (StatusCode, Value) {
    let (status, _, body) = send(
        app,
        caller,
        Method::DELETE,
        &format!("/ai-usage/admin/billing/{month}/close"),
        None,
    )
    .await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

async fn overview(app: &Router, caller: &Caller<'_>, month: &str) -> Value {
    get_json(app, caller, &format!("/ai-usage/admin/billing/{month}")).await
}

async fn csv(app: &Router, caller: &Caller<'_>, month: &str) -> String {
    let (status, _, csv) = get(
        app,
        caller,
        &format!("/ai-usage/admin/billing/{month}/export.csv"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    csv
}

/// An adjustment line as `(adjusted month, snapshot, project, kind, amount,
/// currency, SEK, rate, SEK only)`.
fn adjustment(line: &Value) -> Value {
    json!([
        line["adjustsMonth"],
        line["snapshotId"],
        line["project"]["projectName"],
        line["kind"],
        line["billableAmount"],
        line["billableCurrency"],
        line["convertedAmount"],
        line["exchangeRate"],
        line["sekOnly"],
    ])
}

fn adjustments(bill: &Value) -> Vec<Value> {
    bill["adjustmentLines"]
        .as_array()
        .unwrap()
        .iter()
        .map(adjustment)
        .collect()
}

#[sqlx::test]
async fn close_routes_need_a_plain_admin_session(pool: PgPool) {
    let app = close_app(&pool, rates());
    let June { admin, dev } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let uri = "/ai-usage/admin/billing/2026-06/close";
    for caller in [
        Caller::Token(&admin.token),
        Caller::TokenAndSession(&admin.token, &admin.cookie),
        Caller::Session(&dev.cookie),
    ] {
        for (method, body) in [
            (Method::GET, None),
            (Method::POST, Some(json!({}))),
            (Method::DELETE, None),
        ] {
            let (status, _, _) = send(&app, &caller, method.clone(), uri, body).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method}");
        }
    }
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ai_billing_month_closes")
            .fetch_one(&pool)
            .await
            .unwrap()
            == 0
    );
}

#[sqlx::test]
async fn a_closed_month_keeps_its_bill_and_a_later_change_posts_once_on_the_next_open_month(
    pool: PgPool,
) {
    let app = close_app(&pool, rates());
    let June { admin, dev } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let by = admin.id;
    let admin = Caller::Session(&admin.cookie);

    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-06/close").await;
    assert_eq!(status["state"], "open");
    assert_eq!(status["overAt"], Value::Null);
    assert_eq!(status["readiness"]["ready"], true);
    assert_eq!(
        status["readiness"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|check| json!([check["id"], check["status"]]))
            .collect::<Vec<_>>(),
        [
            json!(["import", "notApplicable"]),
            json!(["dispositions", "notApplicable"]),
            json!(["unknownAccount", "notApplicable"]),
            json!(["repositories", "notApplicable"]),
            json!(["pricing", "passed"]),
            json!(["exchangeRates", "passed"]),
            json!(["uploads", "passed"]),
        ]
    );
    let open = overview(&app, &admin, "2026-06").await;
    assert_eq!(open["close"], Value::Null);
    assert_eq!(open["adjustmentLines"], json!([]));
    assert_eq!(open["adjustedMonths"], json!([]));
    let open_csv = csv(&app, &admin, "2026-06").await;

    let june_close = close(&app, &admin, "2026-06").await;
    let closed = overview(&app, &admin, "2026-06").await;
    assert_eq!(closed["close"]["closeId"], june_close);
    assert_eq!(closed["close"]["adjustmentsPostOn"], "2026-07");
    assert_eq!(closed["close"]["closedBy"], "Admin Example");
    // The snapshot serves the same values as the open month did.
    let without_close = |bill: &Value| {
        let mut bill = bill.clone();
        bill.as_object_mut().unwrap().remove("close");
        bill
    };
    assert_eq!(without_close(&closed), without_close(&open));
    // Its CSV rows name the close.
    let closed_csv = csv(&app, &admin, "2026-06").await;
    let rows = parse_quoted_csv(closed_csv.strip_prefix('\u{feff}').unwrap());
    let column = |name: &str| rows[0].iter().position(|cell| cell == name).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][column("snapshot_id")], june_close.to_string());
    assert_eq!(rows[1][column("adjusts_month")], "");
    let open_rows = parse_quoted_csv(open_csv.strip_prefix('\u{feff}').unwrap());
    assert_eq!(open_rows[1][column("snapshot_id")], "");
    assert_eq!(
        rows[1][..column("snapshot_id")],
        open_rows[1][..column("snapshot_id")]
    );

    // The key is re-mapped to Client B after June closed: June stays as it
    // was invoiced, values and bytes.
    map(&pool, APP, CLIENT_B.0, CLIENT_B.1, by).await;
    assert_eq!(overview(&app, &admin, "2026-06").await, closed);
    assert_eq!(csv(&app, &admin, "2026-06").await, closed_csv);

    // July carries the change: Client A credited, Client B billed, at
    // June's rate.
    let july = overview(&app, &admin, "2026-07").await;
    assert_eq!(
        adjustments(&july),
        [
            json!([
                "2026-06", june_close, "Client A", "api", "-12.50", "USD", "-131.25", "10.50",
                false
            ]),
            json!([
                "2026-06", june_close, "Client B", "api", "12.50", "USD", "131.25", "10.50", false
            ]),
        ]
    );
    assert_eq!(
        july["adjustedMonths"],
        json!([{ "month": "2026-06", "snapshotId": june_close }])
    );
    assert_eq!(july["lines"], json!([]));
    assert_eq!(
        july["totals"]["adjustments"],
        json!([{ "currency": "USD", "amount": "0.00" }])
    );
    assert_eq!(july["totals"]["converted"]["adjustments"], "0.00");
    assert_eq!(july["totals"]["converted"]["invoiced"], "0.00");
    assert_eq!(july["developers"][0]["fullName"], "Ada Lovelace");
    let line = &july["adjustmentLines"][0];
    assert_eq!(
        line["subject"],
        json!({ "kind": "user", "userId": dev.id.as_i32() })
    );
    assert_eq!(line["provider"], "claude");
    assert_eq!(line["meteredKind"], Value::Null);
    assert_eq!(line["credits"], Value::Null);
    assert_eq!(line["missingRate"], false);
    assert_eq!(line["rateChanged"], false);
    // The CSV appends them, signed and unguarded.
    let july_csv = csv(&app, &admin, "2026-07").await;
    assert!(
        july_csv.contains(r#""-12.50","USD","-131.25","10.50""#),
        "{july_csv}"
    );
    let rows = parse_quoted_csv(july_csv.strip_prefix('\u{feff}').unwrap());
    assert_eq!(
        rows[1..]
            .iter()
            .map(|row| {
                (
                    row[column("project")].clone(),
                    row[column("billing_mode")].clone(),
                    row[column("adjusts_month")].clone(),
                    row[column("snapshot_id")].clone(),
                    row[column("source")].clone(),
                )
            })
            .collect::<Vec<_>>(),
        [
            (
                "Client A".to_string(),
                "adjustment".to_string(),
                "2026-06".to_string(),
                june_close.to_string(),
                "estimate".to_string()
            ),
            (
                "Client B".to_string(),
                "adjustment".to_string(),
                "2026-06".to_string(),
                june_close.to_string(),
                "estimate".to_string()
            ),
        ]
    );
    assert_eq!(
        rows[1][column("warning")],
        format!("adjusts 2026-06: its bill changed after it closed (snapshot {june_close})")
    );

    // Closing July freezes them: August shows none, and names both months.
    let july_close = close(&app, &admin, "2026-07").await;
    let august = overview(&app, &admin, "2026-08").await;
    assert_eq!(august["adjustmentLines"], json!([]));
    assert_eq!(
        august["adjustedMonths"],
        json!([
            { "month": "2026-06", "snapshotId": june_close },
            { "month": "2026-07", "snapshotId": july_close },
        ])
    );
    // July serves its frozen adjustments; its developer page too.
    let july = overview(&app, &admin, "2026-07").await;
    assert_eq!(adjustments(&july).len(), 2);
    let drill_down = get_json(
        &app,
        &admin,
        &format!("/ai-usage/admin/billing/2026-07/developers/{}", dev.id),
    )
    .await;
    assert_eq!(drill_down["close"]["closeId"], july_close);
    assert_eq!(drill_down["lines"], json!([]));
    assert_eq!(adjustments(&drill_down), adjustments(&july));
    assert_eq!(
        drill_down["totals"]["adjustments"],
        json!([{ "currency": "USD", "amount": "0.00" }])
    );
    assert_eq!(drill_down["developer"]["fullName"], "Ada Lovelace");
    assert_eq!(drill_down["exchangeRates"], july["exchangeRates"]);
    // June's developer page comes from June's snapshot too.
    let june_drill_down = get_json(
        &app,
        &admin,
        &format!("/ai-usage/admin/billing/2026-06/developers/{}", dev.id),
    )
    .await;
    assert_eq!(june_drill_down["lines"].as_array().unwrap().len(), 1);
    assert_eq!(june_drill_down["totals"]["apiBilledUsd"], "12.50");
    assert_eq!(june_drill_down["dailyUsage"].as_array().unwrap().len(), 1);
}

#[sqlx::test]
async fn a_second_change_posts_only_its_delta_and_every_correction_bills_once(pool: PgPool) {
    let app = close_app(&pool, rates());
    let June { admin, .. } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let by = admin.id;
    let admin = Caller::Session(&admin.cookie);
    let june_close = close(&app, &admin, "2026-06").await;
    map(&pool, APP, CLIENT_B.0, CLIENT_B.1, by).await;
    close(&app, &admin, "2026-07").await;

    // Re-mapped again after July closed: August holds just this change.
    map(&pool, APP, CLIENT_C.0, CLIENT_C.1, by).await;
    let august = overview(&app, &admin, "2026-08").await;
    assert_eq!(
        adjustments(&august),
        [
            json!([
                "2026-06", june_close, "Client B", "api", "-12.50", "USD", "-131.25", "10.50",
                false
            ]),
            json!([
                "2026-06", june_close, "Client C", "api", "12.50", "USD", "131.25", "10.50", false
            ]),
        ]
    );
    close(&app, &admin, "2026-08").await;
    let september = overview(&app, &admin, "2026-09").await;
    assert_eq!(september["adjustmentLines"], json!([]));
    assert_eq!(september["adjustedMonths"].as_array().unwrap().len(), 3);

    // What was invoiced for June, over its snapshot and both months'
    // adjustments, is June recomputed now: 12.50 USD on Client C.
    let mut invoiced: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let june = overview(&app, &admin, "2026-06").await;
    let july = overview(&app, &admin, "2026-07").await;
    let august = overview(&app, &admin, "2026-08").await;
    for line in june["lines"]
        .as_array()
        .unwrap()
        .iter()
        .chain(july["adjustmentLines"].as_array().unwrap())
        .chain(august["adjustmentLines"].as_array().unwrap())
    {
        let amount = |key: &str| line[key].as_str().unwrap().parse::<f64>().unwrap();
        let sum = invoiced
            .entry(line["project"]["projectName"].as_str().unwrap().to_string())
            .or_default();
        sum.0 += amount("billableAmount");
        sum.1 += amount("convertedAmount");
    }
    invoiced.retain(|_, (amount, sek)| amount.abs() > 0.001 || sek.abs() > 0.001);
    assert_eq!(
        invoiced.into_iter().collect::<Vec<_>>(),
        [("Client C".to_string(), (12.5, 131.25))]
    );
}

#[sqlx::test]
async fn a_rate_override_on_a_closed_month_posts_only_the_sek_difference(pool: PgPool) {
    let app = close_app(&pool, rates());
    let June { admin, .. } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let admin = Caller::Session(&admin.cookie);
    let june_close = close(&app, &admin, "2026-06").await;
    let (status, _, body) = send(
        &app,
        &admin,
        Method::PUT,
        "/ai-usage/admin/billing/2026-06/exchange-rates/USD",
        Some(json!({ "rate": "11" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 12.50 USD at 11.00 is 137.50 SEK, 6.25 more than invoiced.
    let july = overview(&app, &admin, "2026-07").await;
    assert_eq!(
        adjustments(&july),
        [json!([
            "2026-06", june_close, "Client A", "api", "0.00", "USD", "6.25", "11.00", true
        ])]
    );
    assert_eq!(july["totals"]["converted"]["adjustments"], "6.25");
    assert_eq!(july["adjustmentLines"][0]["rateChanged"], true);
    let july_csv = csv(&app, &admin, "2026-07").await;
    assert!(july_csv.contains("SEK only: 2026-06's exchange rate changed"));

    // June keeps the rate it closed with and shows the current one apart.
    let june = overview(&app, &admin, "2026-06").await;
    assert_eq!(june["exchangeRates"][0]["rate"], "10.50");
    assert_eq!(june["close"]["currentExchangeRates"][0]["rate"], "11.00");
    assert_eq!(june["close"]["currentExchangeRates"][0]["source"], "admin");
    assert_eq!(june["lines"][0]["convertedAmount"], "131.25");
}

#[sqlx::test]
async fn a_month_reopens_only_once_no_close_holds_adjustments_to_it(pool: PgPool) {
    let app = close_app(&pool, rates());
    let June { admin, .. } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let by = admin.id;
    let admin = Caller::Session(&admin.cookie);
    close(&app, &admin, "2026-06").await;
    map(&pool, APP, CLIENT_B.0, CLIENT_B.1, by).await;
    let july_close = close(&app, &admin, "2026-07").await;

    let (status, body) = reopen(&app, &admin, "2026-06").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["heldBy"], json!(["2026-07"]));
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-06/close").await;
    assert_eq!(status["close"]["heldBy"], json!(["2026-07"]));
    assert_eq!(status["close"]["adjustmentsPostOn"], "2026-08");

    let (status, body) = reopen(&app, &admin, "2026-07").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "open");
    assert_eq!(body["history"][0]["closeId"], july_close);
    assert_eq!(body["history"][0]["reopenedBy"], "Admin Example");
    // July is live again and carries June's adjustments.
    assert_eq!(
        adjustments(&overview(&app, &admin, "2026-07").await).len(),
        2
    );

    let (status, _) = reopen(&app, &admin, "2026-06").await;
    assert_eq!(status, StatusCode::OK);
    let june = overview(&app, &admin, "2026-06").await;
    assert_eq!(june["close"], Value::Null);
    assert_eq!(june["lines"][0]["project"]["projectName"], "Client B");
    assert_eq!(
        adjustments(&overview(&app, &admin, "2026-07").await),
        Vec::<Value>::new()
    );
    let (status, _) = reopen(&app, &admin, "2026-06").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn close_refuses_until_every_failed_check_is_waived(pool: PgPool) {
    // June has no published average yet: its rate is provisional. The
    // uploads end at June's local end, which a month billed on estimates
    // alone accepts.
    let app = close_app(&pool, Arc::new(FakeRates::default().daily("USD", "10.25")));
    let June { admin, .. } = june(&app, &pool, datetime!(2026-06-30 22:00 UTC)).await;
    let admin = Caller::Session(&admin.cookie);

    // The month in progress when the test runs: over at the next month's UTC
    // midnight, after Stockholm's.
    let in_progress = AiBillingMonth::containing(OffsetDateTime::now_utc().date()).unwrap();
    let over_at = format!("{}-01T00:00:00Z", following(in_progress));
    let current = get_json(
        &app,
        &admin,
        &format!("/ai-usage/admin/billing/{in_progress}/close"),
    )
    .await;
    assert_eq!(current["state"], "open");
    assert_eq!(current["overAt"], over_at.as_str());
    assert_eq!(current["readiness"], Value::Null);
    let (status, body) = post_close(&app, &admin, &in_progress.to_string(), json!([])).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["error"],
        format!("{in_progress} is not over yet in UTC and the billing time zone").as_str(),
        "{body}"
    );

    let (status, body) = post_close(&app, &admin, "2026-06", json!([])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["readiness"]["ready"], false);
    let check = |body: &Value, id: &str| -> Value {
        body["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["id"] == id)
            .unwrap()
            .clone()
    };
    let rates_check = check(&body["readiness"], "exchangeRates");
    assert_eq!(rates_check["status"], "failed");
    assert_eq!(rates_check["problems"][0]["kind"], "rateProvisional");
    assert_eq!(rates_check["problems"][0]["currency"], "USD");
    assert_eq!(check(&body["readiness"], "uploads")["status"], "passed");

    for (waivers, why) in [
        (
            json!([{ "check": "everything", "reason": "x" }]),
            "unknown check",
        ),
        (
            json!([{ "check": "exchangeRates", "reason": "   " }]),
            "empty reason",
        ),
        (
            json!([
                { "check": "exchangeRates", "reason": "a" },
                { "check": "exchangeRates", "reason": "b" },
            ]),
            "duplicate",
        ),
        (
            json!([{ "check": "exchangeRates", "reason": "a", "by": "me" }]),
            "unknown field",
        ),
    ] {
        let (status, _) = post_close(&app, &admin, "2026-06", waivers).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}");
    }

    let (status, body) = post_close(
        &app,
        &admin,
        "2026-06",
        json!([
            { "check": "exchangeRates", "reason": "Riksbank publishes June's average on the 5th." },
            { "check": "pricing", "reason": "Nothing to waive here." },
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let recorded = &body["close"]["readiness"];
    assert_eq!(recorded["ready"], true);
    assert_eq!(check(recorded, "exchangeRates")["status"], "waived");
    assert_eq!(
        check(recorded, "exchangeRates")["waiver"],
        json!({ "reason": "Riksbank publishes June's average on the 5th." })
    );
    assert_eq!(check(recorded, "pricing")["status"], "passed");
    assert_eq!(check(recorded, "pricing")["waiver"], Value::Null);

    let (status, body) = post_close(&app, &admin, "2026-06", json!([])).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "2026-06 is already closed");
}

#[sqlx::test]
async fn a_repository_remapped_after_close_posts_on_the_next_month_once(pool: PgPool) {
    let app = app_with(
        &pool,
        FakeRates::august_usd(),
        AiExchangeRateSettings::default(),
        Some(AiMeteredCutover {
            payer: AiProvider::Copilot,
            from: AiBillingMonth::parse("2026-08").unwrap(),
        }),
        REPRICE_FLOOR,
    );
    let CopilotAugust { admin, .. } = copilot_august(&app, &pool).await;
    let admin_id = admin.id;
    let admin = Caller::Session(&admin.cookie);

    // August's import is settled and checked; the unknown account's residual
    // and the unmapped repository fail until waived.
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08/close").await;
    let failed: Vec<(Value, Value)> = status["readiness"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["status"] != "passed")
        .map(|check| (check["id"].clone(), check["problems"].clone()))
        .collect();
    assert_eq!(
        failed,
        [
            (
                json!("unknownAccount"),
                json!([{ "kind": "residual", "day": "2026-08-10", "credits": "5.00" }])
            ),
            (
                json!("repositories"),
                json!([{ "kind": "unmappedRepository", "repository": "gadget-web", "credits": "8.00" }])
            ),
        ]
    );
    let (status, body) = post_close(
        &app,
        &admin,
        "2026-08",
        json!([
            { "check": "repositories", "reason": "gadget-web is an internal tool: Spinit's." },
            { "check": "unknownAccount", "reason": "5 credits of a seat that ended in July." },
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let august_close = body["close"]["closeId"].as_i64().unwrap();
    assert_eq!(
        body["close"]["readiness"]["checks"][2]["waiver"]["reason"],
        "5 credits of a seat that ended in July."
    );
    let august = overview(&app, &admin, "2026-08").await;
    let august_csv = csv(&app, &admin, "2026-08").await;

    // widget-api moves from Client A to Client B after August closed.
    PostgresAiRepositoryMappingRepository::new(db(&pool))
        .upsert(
            AiProvider::Copilot,
            "widget-api",
            &company(),
            &AiMappedProject {
                id: ProjectId::new(CLIENT_B.0),
                name: CLIENT_B.1.to_string(),
            },
            &admin_id,
        )
        .await
        .unwrap();
    assert_eq!(overview(&app, &admin, "2026-08").await, august);
    assert_eq!(csv(&app, &admin, "2026-08").await, august_csv);

    let september = overview(&app, &admin, "2026-09").await;
    let lines: Vec<Value> = september["adjustmentLines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            json!([
                line["subject"],
                line["project"]["projectName"],
                line["kind"],
                line["meteredKind"],
                line["billableAmount"],
                line["credits"],
                line["convertedAmount"],
                line["snapshotId"],
            ])
        })
        .collect();
    let widget = json!({ "kind": "repository", "provider": "copilot", "name": "widget-api" });
    assert_eq!(
        lines,
        [
            json!([
                widget,
                "Client A",
                "metered",
                "repository",
                "-0.12",
                "-12.00",
                "-1.26",
                august_close
            ]),
            json!([
                widget,
                "Client B",
                "metered",
                "repository",
                "0.12",
                "12.00",
                "1.26",
                august_close
            ]),
        ]
    );
    let september_csv = csv(&app, &admin, "2026-09").await;
    assert!(
        september_csv.contains(r#""-0.12","USD","-1.26""#),
        "{september_csv}"
    );
    let rows = parse_quoted_csv(september_csv.strip_prefix('\u{feff}').unwrap());
    let column = |name: &str| rows[0].iter().position(|cell| cell == name).unwrap();
    for row in &rows[1..] {
        assert_eq!(row[column("adjusts_month")], "2026-08");
        assert_eq!(row[column("snapshot_id")], august_close.to_string());
        assert_eq!(row[column("repository")], "widget-api");
        assert_eq!(row[column("period_basis")], "UTC");
        assert_eq!(row[column("source")], "github");
    }

    // September closes with its own (unimported) checks waived; October then
    // shows nothing for August, which September froze.
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-09/close").await;
    let waivers: Vec<Value> = status["readiness"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["status"] == "failed")
        .map(|check| json!({ "check": check["id"], "reason": "September is not imported here." }))
        .collect();
    let (status, body) = post_close(&app, &admin, "2026-09", Value::Array(waivers)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let september_close = body["close"]["closeId"].as_i64().unwrap();
    let october = overview(&app, &admin, "2026-10").await;
    assert_eq!(october["adjustmentLines"], json!([]));
    assert_eq!(
        october["adjustedMonths"],
        json!([
            { "month": "2026-08", "snapshotId": august_close },
            { "month": "2026-09", "snapshotId": september_close },
        ])
    );
}

#[sqlx::test]
async fn a_month_an_import_is_running_or_stopped_in_is_not_ready_on_import(pool: PgPool) {
    let august = AiBillingMonth::parse("2026-08").unwrap();
    let app = app_with(
        &pool,
        FakeRates::august_usd(),
        AiExchangeRateSettings::default(),
        Some(AiMeteredCutover {
            payer: AiProvider::Copilot,
            from: august,
        }),
        REPRICE_FLOOR,
    );
    let CopilotAugust { admin, .. } = copilot_august(&app, &pool).await;
    let admin = Caller::Session(&admin.cookie);
    let import = |status: &Value| readiness_check(&status["readiness"], "import");
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08/close").await;
    assert_eq!(import(&status)["status"], "passed");

    // A run starts importing August, marking its checks first.
    let metered = PostgresAiMeteredUsageRepository::new(db(&pool));
    let lease = metered
        .try_lock_import(AiProvider::Copilot)
        .await
        .unwrap()
        .unwrap();
    let plan = AiImportPlan {
        kind: AiImportKind::Hourly,
        months: vec![(august, AiMonthMode::Changes)],
    };
    let run = metered
        .start_run(AiProvider::Copilot, &plan, datetime!(2026-09-06 02:00 UTC))
        .await
        .unwrap();
    metered
        .mark_checks_importing(AiProvider::Copilot, august, run)
        .await
        .unwrap();

    // While it runs, its marks keep the checks they marked, as the bill
    // reads them: the run itself fails the import, since it may be replacing
    // August's days right now.
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08/close").await;
    let running = json!([{ "kind": "importRunning", "startedAt": "2026-09-06T02:00:00Z" }]);
    assert_eq!(import(&status)["status"], "failed");
    assert_eq!(import(&status)["problems"], running);
    let (code, body) = post_close(&app, &admin, "2026-08", json!([])).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        readiness_check(&body["readiness"], "import")["problems"],
        running
    );

    // No waiver lets it through: with every failed check waived, the
    // import's included, the close is refused until the run finishes, and
    // the import keeps no waiver while the other checks keep theirs.
    let failed: Vec<Value> = status["readiness"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["status"] == "failed")
        .map(|check| check["id"].clone())
        .collect();
    assert!(failed.contains(&json!("import")), "{failed:?}");
    let waivers: Vec<Value> = failed
        .iter()
        .map(|id| json!({ "check": id, "reason": "Closing August anyway." }))
        .collect();
    let (code, body) = post_close(&app, &admin, "2026-08", Value::Array(waivers)).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "an import of 2026-08 is running: try again when it finishes"
    );
    assert_eq!(body["readiness"]["ready"], false);
    let refused = readiness_check(&body["readiness"], "import");
    assert_eq!(refused["status"], "failed");
    assert_eq!(refused["waiver"], Value::Null);
    assert_eq!(refused["problems"], running);
    for id in failed.iter().filter(|id| **id != "import") {
        let check = readiness_check(&body["readiness"], id.as_str().unwrap());
        assert_eq!(check["status"], "waived", "{check}");
    }
    let closes: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_billing_month_closes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(closes, 0);

    // Its process dies before finishing August: every check it left marked
    // is interrupted, the organization's included, and the close refuses.
    lease.release().await.unwrap();
    let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-08/close").await;
    let outcome = json!({
        "kind": "interrupted",
        "error": "the run stopped before finishing this month: interrupted",
    });
    assert_eq!(
        import(&status)["problems"],
        json!([
            { "kind": "accountCheck", "accountId": 1001, "login": "ada-lovelace", "outcome": outcome },
            { "kind": "accountCheck", "accountId": 1004, "login": "kj-contractor", "outcome": outcome },
            { "kind": "orgCheck", "outcome": outcome },
        ])
    );
    let (code, body) = post_close(&app, &admin, "2026-08", json!([])).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["readiness"]["ready"], false);
}

/// The readiness check of `id` in a close status or readiness.
fn readiness_check(readiness: &Value, id: &str) -> Value {
    readiness["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == id)
        .unwrap()
        .clone()
}

/// Waives every failed check of `month` with `reason`.
async fn waive_all(app: &Router, caller: &Caller<'_>, month: &str, reason: &str) -> Value {
    let status = get_json(
        app,
        caller,
        &format!("/ai-usage/admin/billing/{month}/close"),
    )
    .await;
    let waivers: Vec<Value> = status["readiness"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["status"] == "failed")
        .map(|check| json!({ "check": check["id"], "reason": reason }))
        .collect();
    let (status, body) = post_close(app, caller, month, Value::Array(waivers)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

fn without_close(page: &Value) -> Value {
    let mut page = page.clone();
    page.as_object_mut().unwrap().remove("close");
    page
}

#[sqlx::test]
async fn readiness_asks_for_the_reprice_of_an_open_month_before_a_close(pool: PgPool) {
    // Costs reprice from June on: Ada's June usage, sent without a pricing
    // basis, is on an old one.
    let app = app_with(
        &pool,
        rates(),
        AiExchangeRateSettings::default(),
        None,
        datetime!(2026-06-01 00:00 UTC),
    );
    let June { admin, dev } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let admin = Caller::Session(&admin.cookie);
    let june_pricing = || async {
        let status = get_json(&app, &admin, "/ai-usage/admin/billing/2026-06/close").await;
        readiness_check(&status["readiness"], "pricing")
    };
    let pricing = june_pricing().await;
    assert_eq!(pricing["status"], "failed");
    assert_eq!(pricing["problems"][0]["kind"], "oldBasis");

    // July closes. June is still the first open month, so a reprice can
    // still change its usage, and readiness still asks for one.
    close(&app, &admin, "2026-07").await;
    let pricing = june_pricing().await;
    assert_eq!(pricing["status"], "failed", "{pricing}");
    assert_eq!(pricing["problems"][0]["kind"], "oldBasis");

    // Ada's laptop reprices June on its new basis: nothing is left to ask.
    let june_usage = bucket(
        datetime!(2026-06-10 08:00 UTC),
        0,
        APP,
        AiProvider::Claude,
        5,
        Some(12.5),
    );
    let updated = PostgresAiUsageRepository::new(db(&pool))
        .reprice_buckets(
            &dev.id,
            &AiMachineId::parse(LAPTOP).unwrap(),
            &AiPricingBasis::parse("claude-rates@2026-07-01").unwrap(),
            &[&june_usage],
        )
        .await
        .unwrap();
    assert_eq!(updated, 1);
    let pricing = june_pricing().await;
    assert_eq!(pricing["status"], "passed", "{pricing}");
}

/// Copilot's reconciliation over the store, as the router composes it, with
/// costs repriced from `reprice_floor` on.
fn reconciliation_service(
    pool: &PgPool,
    reprice_floor: OffsetDateTime,
) -> impl AiMeteredUsageService {
    AiMeteredUsageServiceImpl::new(
        Arc::new(PostgresAiMeteredUsageRepository::new(db(pool))),
        Arc::new(PostgresAiProviderAccountRepository::new(db(pool))),
        AiMeteredSourceState::Disabled,
        AiProvider::Copilot,
        "example-org".to_string(),
        AiMeteredImportSettings {
            backfill_from: date!(2026 - 08 - 01),
            reprice_floor,
            time_zone: AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
            unattended: false,
        },
        Some(company()),
    )
}

#[sqlx::test]
async fn reconciliation_reads_the_old_basis_from_the_floor_the_bill_reads_it_from(pool: PgPool) {
    // Costs reprice from July on and Copilot bills from its credits from
    // August: Ada's August Copilot usage, sent without a pricing basis, is on
    // an old one.
    let floor = datetime!(2026-07-01 00:00 UTC);
    let app = app_with(
        &pool,
        FakeRates::august_usd(),
        AiExchangeRateSettings::default(),
        Some(AiMeteredCutover {
            payer: AiProvider::Copilot,
            from: AiBillingMonth::parse("2026-08").unwrap(),
        }),
        floor,
    );
    let CopilotAugust { admin, ada } = copilot_august(&app, &pool).await;
    let admin = Caller::Session(&admin.cookie);
    let august = AiBillingMonth::parse("2026-08").unwrap();
    let reconciliation = reconciliation_service(&pool, floor);
    let usage = AiUsageServiceImpl::new(
        Arc::new(PostgresAiUsageRepository::new(db(&pool))),
        floor,
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
    );
    let basis = AiPricingBasis::parse("copilot-rates@2026-09-01").unwrap();
    let machine = AiMachineId::parse(LAPTOP).unwrap();
    // August's tracked usage and the part of it on an old basis, the same
    // in the reconciliation tab and on Ada's card; and the repricing Ada's
    // laptop is asked for.
    let read = || async {
        let everyone = reconciliation
            .reconciliation(august)
            .await
            .unwrap()
            .reconciliation
            .totals;
        let card = reconciliation
            .developer_reconciliation(august, ada.id)
            .await
            .unwrap()
            .reconciliation
            .totals;
        assert_eq!(
            (card.tracked, card.tracked_old_basis),
            (everyone.tracked, everyone.tracked_old_basis)
        );
        let repricing = usage
            .repricing_status(&ada.id, &machine, &basis)
            .await
            .unwrap();
        (
            everyone.tracked,
            everyone.tracked_old_basis,
            repricing.floor,
            repricing.providers[0].from,
        )
    };
    let (tracked, old_basis, discovery_floor, from) = read().await;
    assert!(tracked.nanos() > 0);
    assert_eq!(old_basis, tracked);
    assert_eq!(discovery_floor, floor);
    assert_eq!(from, Some(datetime!(2026-08-10 08:00 UTC)));
    let ada_subject = json!({ "kind": "user", "userId": ada.id.as_i32() });
    let bill = overview(&app, &admin, "2026-08").await;
    assert!(bill["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["billingMode"] == "metered" && line["subject"] == ada_subject)
        .all(|line| line["metered"]["provisionalReasons"] == json!(["oldPricingBasis"])));

    // August closes while July stays open: a reprice can still change
    // August, posting on September, so reconciliation still reads its old
    // basis, as discovery still asks for it.
    waive_all(&app, &admin, "2026-08", "Ada reprices later.").await;
    assert_eq!(read().await, (tracked, old_basis, floor, from));

    // July closes too: September is the first open month, and no reprice
    // may change August any more. Its usage is taken as it is, in
    // reconciliation as in the bill.
    waive_all(&app, &admin, "2026-07", "July bills nothing.").await;
    let september_starts = datetime!(2026-08-31 22:00 UTC);
    assert_eq!(
        read().await,
        (tracked, AiUsdNanos::ZERO, september_starts, None)
    );
    let totals = reconciliation
        .reconciliation(august)
        .await
        .unwrap()
        .reconciliation
        .totals;
    assert_eq!(totals.tracked_before_floor, tracked);
    // Only its flags changed: September posts no adjustment to August.
    let september = overview(&app, &admin, "2026-09").await;
    assert_eq!(september["adjustmentLines"], json!([]), "{september}");
}

#[sqlx::test]
async fn a_closed_developer_page_serves_what_it_served_at_close(pool: PgPool) {
    // Grace's unused EUR plan bills June's overhead without a rate; Ada's
    // lines are in USD only. Ada also used two repositories nobody mapped,
    // and marked the side project personal before June closes.
    let app = close_app(&pool, rates());
    let June { admin, dev } = june(&app, &pool, THROUGH_SEPTEMBER).await;
    let side = "github.com/ada-lovelace/side-project";
    let hobby = "github.com/ada-lovelace/hobby";
    let at = |hour: u8| datetime!(2026-06-10 00:00 UTC).replace_hour(hour).unwrap();
    upload(
        &pool,
        dev.id,
        LAPTOP,
        "work-laptop",
        (datetime!(2026-05-31 22:00 UTC), THROUGH_SEPTEMBER),
        vec![
            bucket(at(8), 0, APP, AiProvider::Claude, 5, Some(12.5)),
            bucket(at(9), 0, side, AiProvider::Claude, 2, Some(2.0)),
            bucket(at(10), 0, hobby, AiProvider::Claude, 1, Some(1.0)),
        ],
    )
    .await;
    let marks = PostgresAiProjectMappingRepository::new(db(&pool));
    marks
        .mark_personal(&dev.id, &AiProjectKey::parse(side).unwrap())
        .await
        .unwrap();
    let grace = test_user(&app, &pool, "grace@example.com", "Grace Hopper", &["User"]).await;
    subscribe(
        &pool,
        grace.id,
        AiProvider::Claude,
        "Claude Pro",
        ("20", "EUR"),
        (date!(2026 - 06 - 01), Some(date!(2026 - 06 - 30))),
    )
    .await;
    let admin = Caller::Session(&admin.cookie);
    let page = |user: UserId| format!("/ai-usage/admin/billing/2026-06/developers/{user}");
    let june = overview(&app, &admin, "2026-06").await;
    assert_eq!(june["missingRates"], json!(["EUR"]));
    let mut open = Vec::new();
    for user in [dev.id, grace.id] {
        open.push(get_json(&app, &admin, &page(user)).await);
    }
    assert_eq!(open[0]["missingRates"], json!([]));
    assert_eq!(open[1]["missingRates"], json!(["EUR"]));
    assert_eq!(open[0]["personalUsage"]["records"], json!(2));
    assert_eq!(open[1]["personalUsage"], Value::Null);

    let (status, body) = post_close(
        &app,
        &admin,
        "2026-06",
        json!([{ "check": "exchangeRates", "reason": "EUR is billed from the CSV." }]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let june_close = body["close"]["closeId"].as_i64().unwrap();
    for (user, open) in [dev.id, grace.id].into_iter().zip(&open) {
        let closed = get_json(&app, &admin, &page(user)).await;
        assert_eq!(closed["close"]["closeId"], june_close);
        assert_eq!(without_close(&closed), without_close(open), "{user}");
    }

    // Marked personal after June closed, the hobby leaves Ada's usage by
    // day, which is live, but neither her bill nor her personal usage: both
    // are as June closed, on her page as on the overview. The change posts
    // once, on July.
    marks
        .mark_personal(&dev.id, &AiProjectKey::parse(hobby).unwrap())
        .await
        .unwrap();
    let closed = get_json(&app, &admin, &page(dev.id)).await;
    // Unmapped keys are withheld (null): the hobby's records leave.
    let rows = |page: &Value| -> BTreeSet<(Option<String>, i64)> {
        page["dailyUsage"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["projectKey"].as_str().map(str::to_string),
                    row["usage"]["records"].as_i64().unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(
        rows(&open[0]),
        BTreeSet::from([(Some(APP.to_string()), 5), (None, 1)])
    );
    assert_eq!(rows(&closed), BTreeSet::from([(Some(APP.to_string()), 5)]));
    let frozen = |page: &Value| {
        let mut page = without_close(page);
        page.as_object_mut().unwrap().remove("dailyUsage");
        page
    };
    assert_eq!(frozen(&closed), frozen(&open[0]));
    let june = overview(&app, &admin, "2026-06").await;
    assert_eq!(
        june["personalUsage"],
        json!([{ "userId": dev.id.as_i32(), "usage": closed["personalUsage"] }])
    );
    // Grace's overhead still waits for June's EUR rate.
    let july = overview(&app, &admin, "2026-07").await;
    assert_eq!(
        adjustments(&july),
        [
            json!(["2026-06", june_close, null, "api", "-1.00", "USD", "-10.50", "10.50", false]),
            json!(["2026-06", june_close, null, "overhead", "0.00", "EUR", null, null, false]),
        ]
    );
}

#[sqlx::test]
async fn an_account_linked_after_close_posts_its_credits_under_its_login(pool: PgPool) {
    let app = app_with(
        &pool,
        FakeRates::august_usd(),
        AiExchangeRateSettings::default(),
        Some(AiMeteredCutover {
            payer: AiProvider::Copilot,
            from: AiBillingMonth::parse("2026-08").unwrap(),
        }),
        REPRICE_FLOOR,
    );
    let CopilotAugust { admin, ada } = copilot_august(&app, &pool).await;
    let by = admin.id;
    let admin = Caller::Session(&admin.cookie);
    // Ada's account loses its link before August closes, and only GitHub's
    // credits bill her: her usage waits for a link, with no line, and her
    // account's credits bill to Spinit as unlinked.
    sqlx::query("DELETE FROM ai_provider_account_dispositions WHERE account_id = 1001")
        .execute(&pool)
        .await
        .unwrap();
    upload_ada_copilot_only(&pool, ada.id).await;
    // The overview fetches August's USD rate, which every page lists.
    overview(&app, &admin, "2026-08").await;
    let page = format!("/ai-usage/admin/billing/2026-08/developers/{}", ada.id);
    let open = get_json(&app, &admin, &page).await;
    assert_eq!(open["lines"], json!([]));
    assert_eq!(
        open["metered"]["unbilled"]["trackedUnlinked"]["records"],
        json!(7 + 2)
    );

    let closed = waive_all(&app, &admin, "2026-08", "Ada's account is linked later.").await;
    let august_close = closed["close"]["closeId"].as_i64().unwrap();
    // Her closed page still says what her usage waits for.
    assert_eq!(
        without_close(&get_json(&app, &admin, &page).await),
        without_close(&open)
    );

    // Linked again after August closed: September moves the account's
    // credits from Unlinked account to Ada, and its CSV names the account.
    PostgresAiProviderAccountRepository::new(db(&pool))
        .insert_disposition(
            AiProvider::Copilot,
            AiProviderAccountId::new(1001).unwrap(),
            &AiAccountDispositionTerms {
                target: AiAccountTarget::User(ada.id),
                period: AiAccountPeriod::new(date!(2026 - 07 - 01), None).unwrap(),
            },
            &by,
        )
        .await
        .unwrap();
    let september = overview(&app, &admin, "2026-09").await;
    let unlinked: Vec<&Value> = september["adjustmentLines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|line| line["meteredKind"] == "unlinkedAccount")
        .collect();
    assert_eq!(unlinked.len(), 1, "{september}");
    assert_eq!(
        unlinked[0]["subject"],
        json!({ "kind": "providerAccount", "provider": "copilot", "accountId": 1001, "login": "ada-lovelace" })
    );
    let september_csv = csv(&app, &admin, "2026-09").await;
    let rows = parse_quoted_csv(september_csv.strip_prefix('\u{feff}').unwrap());
    let column = |name: &str| rows[0].iter().position(|cell| cell == name).unwrap();
    let account_rows: Vec<&Vec<String>> = rows[1..]
        .iter()
        .filter(|row| row[column("project")] == "Unlinked account")
        .collect();
    assert_eq!(account_rows.len(), 1, "{september_csv}");
    let row = account_rows[0];
    assert_eq!(row[column("billing_mode")], "adjustment");
    assert_eq!(row[column("developer")], "");
    assert_eq!(row[column("github_login")], "ada-lovelace");
    assert_eq!(row[column("adjusts_month")], "2026-08");
    assert_eq!(row[column("snapshot_id")], august_close.to_string());
    assert!(row[column("billable_amount")].starts_with('-'), "{row:?}");
}
