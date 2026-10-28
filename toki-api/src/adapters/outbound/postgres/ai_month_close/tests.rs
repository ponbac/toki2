use serde_json::json;
use sqlx::PgPool;
use time::macros::{date, datetime};

use super::*;
use crate::{
    adapters::outbound::postgres::{
        PostgresAiBillingRepository, PostgresAiMeteredUsageRepository, PostgresAiUsageRepository,
    },
    domain::{
        models::{
            reprice_floor, AiAdjustedMonth, AiBillingSubject, AiCoverageStatus, AiMachine,
            AiMachineId, AiMeteredAccountReason, AiMeteredChargeKind, AiNewSnapshotLine,
            AiPartialBucketCost, AiPricing, AiPricingBasis, AiPricingStatus, AiProviderAccountId,
            AiProviderCoverage, AiProviderLogin, AiTokenCounts, AiUnassignedKind, AiUsageBucket,
            AiUsageTimeZone, AiUsageUpload, AiUsageWindow, ProjectId,
        },
        ports::outbound::{AiBillingRepository, AiMeteredUsageRepository, AiUsageRepository},
    },
};

fn db(pool: &PgPool) -> DbPool {
    DbPool::from(pool.clone())
}

fn month(raw: &str) -> AiBillingMonth {
    AiBillingMonth::parse(raw).unwrap()
}

async fn admin(pool: &PgPool) -> UserId {
    UserId::new(
        sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token, roles)
             VALUES ('admin@example.com', 'Admin Example', '', '', ARRAY['Admin'])
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap(),
    )
}

fn usd() -> AiCurrency {
    AiCurrency::usd()
}

fn client_a() -> AiMappedProject {
    AiMappedProject {
        id: ProjectId::new("101"),
        name: "Client A".to_string(),
    }
}

/// Ada's API line on Client A.
fn api(amount: i64, sek: i64) -> AiBilledEntry {
    let subject = AiBillingSubject::User(UserId::new(1));
    AiBilledEntry {
        key: AiAdjustmentKey {
            subject: AiKeySubject::of(&subject),
            project: AiKeyProject::Project(ProjectId::new("101")),
            kind: AiAdjustedKind::Api,
            currency: usd(),
            payer: AiKeyPayer::Provider(AiProvider::Claude),
        },
        subject,
        provider: AiProvider::Claude,
        project: Some(client_a()),
        unassigned: None,
        amount: Some(amount),
        sek: AiEntrySek::Known(sek),
        rate: AiExchangeRateValue::parse("10.5"),
        credits: None,
    }
}

/// An unlinked account's metered line.
fn unlinked(amount: i64) -> AiBilledEntry {
    let subject = AiBillingSubject::ProviderAccount {
        provider: AiProvider::Copilot,
        account_id: AiProviderAccountId::new(1001).unwrap(),
        login: AiProviderLogin::parse("ada-lovelace").unwrap(),
    };
    AiBilledEntry {
        key: AiAdjustmentKey {
            subject: AiKeySubject::of(&subject),
            project: AiKeyProject::None,
            kind: AiAdjustedKind::Metered(AiMeteredChargeKind::Account(
                AiMeteredAccountReason::Unlinked,
            )),
            currency: usd(),
            payer: AiKeyPayer::Provider(AiProvider::Copilot),
        },
        subject,
        provider: AiProvider::Copilot,
        project: None,
        unassigned: None,
        amount: Some(amount),
        sek: AiEntrySek::Unknown,
        rate: None,
        credits: Some(amount),
    }
}

/// An unmapped repository's metered line.
fn unmapped_repository(amount: i64) -> AiBilledEntry {
    let subject = AiBillingSubject::Repository {
        provider: AiProvider::Copilot,
        name: "Gadget-Web".to_string(),
    };
    AiBilledEntry {
        key: AiAdjustmentKey {
            subject: AiKeySubject::of(&subject),
            project: AiKeyProject::Unassigned(AiUnassignedKind::UnmappedRepository),
            kind: AiAdjustedKind::Metered(AiMeteredChargeKind::Repository),
            currency: usd(),
            payer: AiKeyPayer::Provider(AiProvider::Copilot),
        },
        subject,
        provider: AiProvider::Copilot,
        project: None,
        unassigned: Some(AiUnassignedKind::UnmappedRepository),
        amount: Some(amount),
        sek: AiEntrySek::Known(amount * 10),
        rate: AiExchangeRateValue::parse("10"),
        credits: Some(amount),
    }
}

/// A Codex subscription's share.
fn share(amount: i64) -> AiBilledEntry {
    let subject = AiBillingSubject::User(UserId::new(1));
    AiBilledEntry {
        key: AiAdjustmentKey {
            subject: AiKeySubject::of(&subject),
            project: AiKeyProject::Project(ProjectId::new("101")),
            kind: AiAdjustedKind::Subscription,
            currency: AiCurrency::billing(),
            payer: AiKeyPayer::Subscription(crate::domain::models::AiSubscriptionId::new(5)),
        },
        subject,
        provider: AiProvider::Codex,
        project: Some(client_a()),
        unassigned: None,
        amount: Some(amount),
        sek: AiEntrySek::Known(amount),
        rate: None,
        credits: None,
    }
}

fn line(
    position: u32,
    csv_position: u32,
    adjusts: Option<AiAdjustedMonth>,
    entry: AiBilledEntry,
) -> AiNewSnapshotLine {
    AiNewSnapshotLine {
        position,
        csv_position,
        adjusts,
        line_json: json!({ "position": position }),
        csv_row: vec![format!("row {position}"), "-1.50".to_string()],
        entry,
    }
}

async fn store(
    repository: &PostgresAiMonthCloseRepository,
    month: AiBillingMonth,
    by: UserId,
    lines: Vec<AiNewSnapshotLine>,
) -> Result<AiMonthClose, AiMonthCloseError> {
    let id = repository.reserve_id().await.unwrap();
    repository
        .store(&AiNewClose {
            id,
            month,
            by,
            readiness_json: json!({ "ready": true, "checks": [] }),
            bill_json: json!({ "month": month.to_string() }),
            developer_json: json!({ "1": { "subscriptions": [] } }),
            csv_header: vec!["month".to_string(), "billable_amount".to_string()],
            projection_version: 1,
            lines,
        })
        .await
}

#[sqlx::test]
async fn a_close_round_trips_its_snapshot(pool: PgPool) {
    let by = admin(&pool).await;
    let repository = PostgresAiMonthCloseRepository::new(db(&pool));
    let june = store(
        &repository,
        month("2026-06"),
        by,
        vec![
            line(0, 1, None, api(1_250, 13_125)),
            line(1, 0, None, unlinked(500)),
            line(2, 2, None, unmapped_repository(8)),
            line(3, 3, None, share(55_000)),
        ],
    )
    .await
    .unwrap();
    assert_eq!(june.month, month("2026-06"));
    assert_eq!(june.closed_by.as_deref(), Some("Admin Example"));
    assert!(june.is_active());

    // July holds a signed adjustment to June.
    let adjusts = AiAdjustedMonth {
        month: june.month,
        close_id: june.id,
    };
    let july = store(
        &repository,
        month("2026-07"),
        by,
        vec![
            line(0, 0, None, api(300, 3_150)),
            line(1, 1, Some(adjusts), api(-1_250, -13_125)),
        ],
    )
    .await
    .unwrap();

    let own = repository.own_entries(june.id).await.unwrap();
    assert_eq!(
        own,
        [
            api(1_250, 13_125),
            unlinked(500),
            unmapped_repository(8),
            share(55_000)
        ]
    );
    let frozen = repository
        .frozen_adjustments(month("2026-06"))
        .await
        .unwrap();
    assert_eq!(frozen, [api(-1_250, -13_125)]);
    assert_eq!(
        repository.months_adjusting(month("2026-06")).await.unwrap(),
        [month("2026-07")]
    );
    assert!(repository
        .months_adjusting(month("2026-07"))
        .await
        .unwrap()
        .is_empty());

    let snapshot = repository.snapshot(july.id).await.unwrap();
    assert_eq!(snapshot.projection_version, 1);
    assert_eq!(snapshot.bill_json, json!({ "month": "2026-07" }));
    assert_eq!(snapshot.csv_header, ["month", "billable_amount"]);
    assert_eq!(
        snapshot
            .lines
            .iter()
            .map(|line| (line.csv_position, line.adjusts, line.line_json.clone()))
            .collect::<Vec<_>>(),
        [
            (0, None, json!({ "position": 0 })),
            (1, Some(month("2026-06")), json!({ "position": 1 })),
        ]
    );
    assert_eq!(snapshot.lines[1].csv_row, ["row 1", "-1.50"]);
    let rate: Option<String> = sqlx::query_scalar(
        "SELECT rate::text FROM ai_billing_snapshot_lines WHERE close_id = $1 AND position = 1",
    )
    .bind(july.id.as_i64())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rate.as_deref(), Some("10.500000"));
    assert_eq!(
        repository
            .active()
            .await
            .unwrap()
            .iter()
            .map(|close| close.month)
            .collect::<Vec<_>>(),
        [month("2026-06"), month("2026-07")]
    );
}

#[sqlx::test]
async fn a_month_has_one_active_close_and_keeps_its_history(pool: PgPool) {
    let by = admin(&pool).await;
    let repository = PostgresAiMonthCloseRepository::new(db(&pool));
    let first = store(
        &repository,
        month("2026-06"),
        by,
        vec![line(0, 0, None, api(1, 11))],
    )
    .await
    .unwrap();
    assert!(matches!(
        store(&repository, month("2026-06"), by, Vec::new()).await,
        Err(AiMonthCloseError::AlreadyClosed { .. })
    ));
    repository.reopen(month("2026-06"), &by).await.unwrap();
    assert!(matches!(
        repository.reopen(month("2026-06"), &by).await,
        Err(AiMonthCloseError::NotClosed { .. })
    ));
    assert!(repository.active().await.unwrap().is_empty());
    let second = store(
        &repository,
        month("2026-06"),
        by,
        vec![line(0, 0, None, api(2, 21))],
    )
    .await
    .unwrap();
    let history = repository.history(month("2026-06")).await.unwrap();
    assert_eq!(
        history.iter().map(|close| close.id).collect::<Vec<_>>(),
        [second.id, first.id]
    );
    assert!(history[0].is_active());
    assert_eq!(history[1].reopened_by.as_deref(), Some("Admin Example"));
    assert!(history[1].reopened_at.unwrap() >= history[1].closed_at);
}

#[sqlx::test]
async fn frozen_adjustments_are_read_from_active_closes_only(pool: PgPool) {
    let by = admin(&pool).await;
    let repository = PostgresAiMonthCloseRepository::new(db(&pool));
    let june = store(
        &repository,
        month("2026-06"),
        by,
        vec![line(0, 0, None, api(1_000, 10_500))],
    )
    .await
    .unwrap();
    let adjusts = AiAdjustedMonth {
        month: june.month,
        close_id: june.id,
    };
    store(
        &repository,
        month("2026-07"),
        by,
        vec![line(0, 0, Some(adjusts), api(-1_000, -10_500))],
    )
    .await
    .unwrap();
    repository.reopen(month("2026-07"), &by).await.unwrap();
    assert!(repository
        .frozen_adjustments(month("2026-06"))
        .await
        .unwrap()
        .is_empty());
    assert!(repository
        .months_adjusting(month("2026-06"))
        .await
        .unwrap()
        .is_empty());

    // A snapshot cannot hold adjustments to a re-opened close.
    repository.reopen(month("2026-06"), &by).await.unwrap();
    assert!(matches!(
        store(
            &repository,
            month("2026-08"),
            by,
            vec![line(0, 0, Some(adjusts), api(-1_000, -10_500))],
        )
        .await,
        Err(AiMonthCloseError::Busy)
    ));
    assert!(repository
        .history(month("2026-08"))
        .await
        .unwrap()
        .is_empty());
}

/// Inserts a raw snapshot row into close `close_id`, for the constraints.
#[allow(clippy::too_many_arguments)]
async fn raw_line(
    pool: &PgPool,
    close_id: i64,
    project: &str,
    project_name: Option<&str>,
    kind: &str,
    subscription_id: Option<i32>,
    amount: Option<i64>,
    credits: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ai_billing_snapshot_lines (
             close_id, position, csv_position, subject, project, project_name, kind, currency,
             provider, subscription_id, amount, credits, line_json, csv_row
         )
         VALUES ($1, 9, 9, 'user:1', $2, $3, $4, 'USD', 'claude', $5, $6, $7, '{}', ARRAY['x'])",
    )
    .bind(close_id)
    .bind(project)
    .bind(project_name)
    .bind(kind)
    .bind(subscription_id)
    .bind(amount)
    .bind(credits)
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test]
async fn snapshot_rows_are_checked_by_the_store(pool: PgPool) {
    let by = admin(&pool).await;
    let repository = PostgresAiMonthCloseRepository::new(db(&pool));
    let june = store(&repository, month("2026-06"), by, Vec::new())
        .await
        .unwrap()
        .id
        .as_i64();
    let constraint = |result: Result<(), sqlx::Error>| {
        result
            .unwrap_err()
            .as_database_error()
            .and_then(|error| error.constraint().map(str::to_string))
    };
    // (case, project key, project name, kind, subscription, own cents,
    // credits, the constraint that refuses it)
    for (case, key, name, kind, subscription, own, credits, expected) in [
        (
            "a negative own amount",
            "project:101",
            Some("Client A"),
            "api",
            None,
            Some(-1),
            None,
            "ai_billing_snapshot_lines_sign_check",
        ),
        (
            "a subscription on an API line",
            "project:101",
            Some("Client A"),
            "api",
            Some(5),
            Some(1),
            None,
            "ai_billing_snapshot_lines_subscription_check",
        ),
        (
            "credits on an API line",
            "project:101",
            Some("Client A"),
            "api",
            None,
            Some(1),
            Some(1),
            "ai_billing_snapshot_lines_credits_check",
        ),
        (
            "a project name without a project",
            "none",
            Some("Client A"),
            "api",
            None,
            Some(1),
            None,
            "ai_billing_snapshot_lines_project_check",
        ),
        (
            "an unknown kind",
            "none",
            None,
            "refund",
            None,
            Some(1),
            None,
            "ai_billing_snapshot_lines_kind_check",
        ),
    ] {
        let result = raw_line(&pool, june, key, name, kind, subscription, own, credits).await;
        assert_eq!(constraint(result).as_deref(), Some(expected), "{case}");
    }
    // A valid row is stored.
    raw_line(
        &pool,
        june,
        "project:101",
        Some("Client A"),
        "api",
        None,
        None,
        None,
    )
    .await
    .unwrap();
}

#[sqlx::test]
async fn one_close_runs_at_a_time_and_ids_are_reserved(pool: PgPool) {
    let repository = PostgresAiMonthCloseRepository::new(db(&pool));
    let lease = repository.try_lock().await.unwrap().unwrap();
    assert!(repository.try_lock().await.unwrap().is_none());
    lease.release().await.unwrap();
    let again = repository.try_lock().await.unwrap();
    assert!(again.is_some());
    drop(again);
    let first = repository.reserve_id().await.unwrap();
    let second = repository.reserve_id().await.unwrap();
    assert!(second > first);
}

#[sqlx::test]
async fn the_reprice_floor_is_where_the_first_open_month_starts(pool: PgPool) {
    let by = admin(&pool).await;
    let closes = PostgresAiMonthCloseRepository::new(db(&pool));
    let usage = PostgresAiUsageRepository::new(db(&pool));
    let metered = PostgresAiMeteredUsageRepository::new(db(&pool));
    let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
    let new_york = AiUsageTimeZone::parse("America/New_York").unwrap();
    // Costs reprice from 10 May on. Every store answers alike.
    let configured = datetime!(2026-05-10 00:00 UTC);
    let open_from = |time_zone: AiUsageTimeZone, configured: OffsetDateTime| {
        let (closes, usage, metered) = (&closes, &usage, &metered);
        async move {
            let from = closes.open_from(&time_zone, configured).await.unwrap();
            assert_eq!(usage.open_from(&time_zone, configured).await.unwrap(), from);
            assert_eq!(
                metered.open_from(&time_zone, configured).await.unwrap(),
                from
            );
            from
        }
    };
    // May is open: it starts at its Stockholm midnight, two hours before
    // its UTC month, and the configured floor is later.
    assert_eq!(
        open_from(stockholm.clone(), configured).await,
        datetime!(2026-04-30 22:00 UTC)
    );
    // A close before the floor's month, or of a month after an open one,
    // leaves May the first open month.
    for closed in ["2026-04", "2026-06"] {
        store(&closes, month(closed), by, Vec::new()).await.unwrap();
    }
    assert_eq!(
        open_from(stockholm.clone(), configured).await,
        datetime!(2026-04-30 22:00 UTC)
    );
    // May closes: July is the first open month, from the earlier of UTC and
    // local midnight on 1 July (New York's is four hours after UTC's).
    store(&closes, month("2026-05"), by, Vec::new())
        .await
        .unwrap();
    assert_eq!(
        open_from(stockholm.clone(), configured).await,
        datetime!(2026-06-30 22:00 UTC)
    );
    assert_eq!(
        open_from(new_york.clone(), configured).await,
        datetime!(2026-07-01 00:00 UTC)
    );
    let floor = reprice_floor(configured, open_from(stockholm.clone(), configured).await);
    assert_eq!(floor, datetime!(2026-06-30 22:00 UTC));
    // June re-opens and is the first open month again.
    closes.reopen(month("2026-06"), &by).await.unwrap();
    assert_eq!(
        open_from(stockholm, configured).await,
        datetime!(2026-05-31 22:00 UTC)
    );
    // A floor in New York's June but UTC's July counts from June: its local
    // days from the floor on are open.
    assert_eq!(
        open_from(new_york, datetime!(2026-07-01 02:00 UTC)).await,
        datetime!(2026-06-01 00:00 UTC)
    );
}

const LAPTOP: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

fn bucket(hour_start: OffsetDateTime, provider: AiProvider, cost: f64) -> AiUsageBucket {
    AiUsageBucket {
        billing_provider: None,
        hour_start,
        session_key: format!("{:032x}", hour_start.unix_timestamp()),
        project_key: "github.com/example/app".to_string(),
        provider,
        model: "example-model".to_string(),
        tokens: AiTokenCounts {
            input: 10,
            ..AiTokenCounts::default()
        },
        records: 1,
        estimated_cost_usd: Some(cost),
        unpriced_records: 0,
    }
}

async fn upload(
    pool: &PgPool,
    user: UserId,
    window: (OffsetDateTime, OffsetDateTime),
    buckets: Vec<AiUsageBucket>,
    providers: &[AiProvider],
    basis: Option<&str>,
) {
    let upload = AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(LAPTOP).unwrap(),
            label: "work-laptop".to_string(),
            client_version: "0.10.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        AiUsageWindow::new(window.0, window.1).unwrap(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: None,
            source: "https://example.com/prices.json".to_string(),
        },
        buckets,
        providers
            .iter()
            .map(|&provider| AiProviderCoverage {
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
        AiPartialBucketCost::PricedSubtotal,
    )
    .unwrap()
    .with_pricing_basis(basis.map(|basis| AiPricingBasis::parse(basis).unwrap()));
    PostgresAiUsageRepository::new(db(pool))
        .replace_window(&user, &upload)
        .await
        .unwrap();
}

#[sqlx::test]
async fn readiness_reads_old_basis_estimates_and_logged_syncs(pool: PgPool) {
    let ada = admin(&pool).await;
    let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
    let billing = PostgresAiBillingRepository::new(db(&pool));
    // October on an old basis (Claude) and none (Codex), September before the
    // floor, then Claude repriced: the machine's current basis is the new one.
    upload(
        &pool,
        ada,
        (
            datetime!(2026-09-29 22:00 UTC),
            datetime!(2026-10-20 22:00 UTC),
        ),
        vec![
            bucket(datetime!(2026-09-30 08:00 UTC), AiProvider::Claude, 1.0),
            bucket(datetime!(2026-10-02 08:00 UTC), AiProvider::Claude, 2.0),
            bucket(datetime!(2026-10-09 21:00 UTC), AiProvider::Claude, 1.5),
        ],
        &[AiProvider::Claude],
        Some("prices@2026-09"),
    )
    .await;
    upload(
        &pool,
        ada,
        (
            datetime!(2026-10-10 00:00 UTC),
            datetime!(2026-10-11 00:00 UTC),
        ),
        vec![bucket(
            datetime!(2026-10-10 08:00 UTC),
            AiProvider::Codex,
            0.25,
        )],
        &[AiProvider::Codex],
        Some("prices@2026-10"),
    )
    .await;
    let october = month("2026-10").dates();
    let old = billing
        .old_basis_usage(
            october,
            &stockholm,
            None,
            None,
            datetime!(2026-10-01 00:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(
        old.iter()
            .map(|usage| (
                usage.provider,
                usage.buckets,
                usage.first_day,
                usage.last_day,
                usage.estimate.to_decimal(2)
            ))
            .collect::<Vec<_>>(),
        // The bucket at 21:00 UTC on the 9th is 23:00 on the 9th in Stockholm.
        [(
            AiProvider::Claude,
            2,
            date!(2026 - 10 - 02),
            date!(2026 - 10 - 09),
            "3.50".to_string()
        )]
    );
    assert_eq!(old[0].developer.full_name, "Admin Example");
    assert_eq!(old[0].label, "work-laptop");
    // September, before the floor, is never repriced: nothing is old there.
    assert!(billing
        .old_basis_usage(
            month("2026-09").dates(),
            &stockholm,
            None,
            None,
            datetime!(2026-10-01 00:00 UTC),
        )
        .await
        .unwrap()
        .is_empty());

    // Logged syncs overlapping an instant range, every machine's.
    let logged = billing
        .coverage_intervals(
            datetime!(2026-10-10 22:00 UTC),
            datetime!(2026-10-11 00:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(
        logged
            .iter()
            .map(|(machine, interval)| (machine.to_string(), interval.provider))
            .collect::<Vec<_>>(),
        [
            (LAPTOP.to_string(), AiProvider::Claude),
            (LAPTOP.to_string(), AiProvider::Codex)
        ]
    );
    assert!(billing
        .coverage_intervals(
            datetime!(2026-10-21 00:00 UTC),
            datetime!(2026-10-22 00:00 UTC),
        )
        .await
        .unwrap()
        .is_empty());
}
