use sqlx::PgPool;
use time::{macros::datetime, OffsetDateTime};

use crate::domain::models::{
    AiCoverageStatus, AiMachine, AiMachineId, AiPartialBucketCost, AiPricing, AiPricingBasis,
    AiPricingStatus, AiProvider, AiProviderCoverage, AiProviderHint, AiProviderRepricing,
    AiTokenCounts, AiUsageBucket, AiUsageWindow,
};

use super::*;

const MACHINE_A: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
const MACHINE_B: &str = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
const PROJECT: &str = "github.com/example/app";
const WINDOW: (OffsetDateTime, OffsetDateTime) = (
    datetime!(2026-09-22 06:00 UTC),
    datetime!(2026-09-22 12:00 UTC),
);
const BOTH_OK: &[(AiProvider, AiCoverageStatus)] = &[
    (AiProvider::Claude, AiCoverageStatus::Ok),
    (AiProvider::Codex, AiCoverageStatus::Ok),
];

fn repository(pool: &PgPool) -> PostgresAiUsageRepository {
    PostgresAiUsageRepository::new(crate::db::DbPool::from(pool.clone()))
}

async fn insert_user(pool: &PgPool, email: &str) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ($1, 'Test User', '', '')
         RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

fn bucket(hour_start: OffsetDateTime, records: i64, cost: Option<f64>) -> AiUsageBucket {
    AiUsageBucket {
        billing_provider: None,
        hour_start,
        session_key: "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f".to_string(),
        project_key: PROJECT.to_string(),
        provider: AiProvider::Claude,
        model: "example-model".to_string(),
        tokens: AiTokenCounts {
            input: 10 * records,
            cache_read: 100 * records,
            cache_write: 5 * records,
            output: 7 * records,
        },
        records,
        estimated_cost_usd: cost,
        unpriced_records: if cost.is_none() { records } else { 0 },
    }
}

fn codex_bucket(hour_start: OffsetDateTime, records: i64) -> AiUsageBucket {
    AiUsageBucket {
        billing_provider: None,
        provider: AiProvider::Codex,
        ..bucket(hour_start, records, Some(0.01))
    }
}

fn hint(hour_start: OffsetDateTime) -> AiProviderHint {
    AiProviderHint {
        provider: AiProvider::Codex,
        hour_start,
        plan: "pro".to_string(),
    }
}

/// An upload that replaces Claude and Codex usage.
fn upload(
    machine_id: &str,
    label: &str,
    window: (OffsetDateTime, OffsetDateTime),
    buckets: Vec<AiUsageBucket>,
    hints: Vec<AiProviderHint>,
) -> AiUsageUpload {
    upload_covering(machine_id, label, window, BOTH_OK, buckets, hints)
}

fn upload_covering(
    machine_id: &str,
    label: &str,
    window: (OffsetDateTime, OffsetDateTime),
    coverage: &[(AiProvider, AiCoverageStatus)],
    buckets: Vec<AiUsageBucket>,
    hints: Vec<AiProviderHint>,
) -> AiUsageUpload {
    AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(machine_id).unwrap(),
            label: label.to_string(),
            client_version: "0.2.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        AiUsageWindow::new(window.0, window.1).unwrap(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: Some(datetime!(2026-09-22 07:00 UTC)),
            source: "https://example.com/prices.json".to_string(),
        },
        buckets,
        coverage
            .iter()
            .map(|&(provider, status)| AiProviderCoverage {
                provider,
                status,
                files: 3,
                unreadable: 0,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 1,
            })
            .collect(),
        hints,
        AiPartialBucketCost::PricedSubtotal,
    )
    .unwrap()
}

/// Every stored row, without the times that record when a sync happened.
async fn snapshot(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT row_to_json(b)::text FROM (
             SELECT * FROM ai_usage_buckets
             ORDER BY machine_id, hour_start, session_key, project_key, provider, model
         ) b
         UNION ALL
         SELECT row_to_json(h)::text FROM (
             SELECT * FROM ai_usage_provider_hints
             ORDER BY machine_id, hour_start, provider, plan
         ) h
         UNION ALL
         SELECT (to_jsonb(c) - 'reported_at')::text FROM (
             SELECT * FROM ai_usage_machine_coverage ORDER BY machine_id, provider
         ) c
         UNION ALL
         SELECT (to_jsonb(m) - 'last_synced_at')::text FROM (
             SELECT * FROM ai_usage_machines ORDER BY id
         ) m",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Stored `(hour_start, provider, records)` for a machine.
async fn stored_buckets(pool: &PgPool, machine_id: &str) -> Vec<(OffsetDateTime, String, i64)> {
    sqlx::query_as(
        "SELECT hour_start, provider, records FROM ai_usage_buckets
         WHERE machine_id = $1::uuid
         ORDER BY hour_start, provider",
    )
    .bind(machine_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Stored `(machine_id, hour_start)` of every hint.
async fn stored_hints(pool: &PgPool) -> Vec<(String, OffsetDateTime)> {
    sqlx::query_as(
        "SELECT machine_id::text, hour_start FROM ai_usage_provider_hints
         ORDER BY machine_id, hour_start",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn reuploading_the_same_payload_leaves_identical_state(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let payload = upload(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        vec![
            bucket(datetime!(2026-09-22 08:00 UTC), 3, Some(0.0421)),
            bucket(datetime!(2026-09-22 09:00 UTC), 2, None),
            codex_bucket(datetime!(2026-09-22 09:00 UTC), 1),
        ],
        vec![hint(datetime!(2026-09-22 08:00 UTC))],
    );

    assert_eq!(repository.replace_window(&user, &payload).await.unwrap(), 3);
    let first = snapshot(&pool).await;
    assert_eq!(repository.replace_window(&user, &payload).await.unwrap(), 3);

    assert_eq!(snapshot(&pool).await, first);
    assert_eq!(
        first.len(),
        7,
        "three buckets, one hint, two coverage rows and one machine"
    );
}

#[sqlx::test]
async fn another_users_upload_is_rejected_without_changes(pool: PgPool) {
    let repository = repository(&pool);
    let owner = insert_user(&pool, "owner@example.com").await;
    let intruder = insert_user(&pool, "intruder@example.com").await;
    let owned = upload(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        vec![bucket(datetime!(2026-09-22 08:00 UTC), 3, Some(0.5))],
        Vec::new(),
    );
    repository.replace_window(&owner, &owned).await.unwrap();
    let before = snapshot(&pool).await;

    let hijack = upload(MACHINE_A, "other-laptop", WINDOW, Vec::new(), Vec::new());
    assert!(matches!(
        repository.replace_window(&intruder, &hijack).await,
        Err(AiUsageError::MachineOwnedByAnotherUser)
    ));

    assert_eq!(snapshot(&pool).await, before);
}

#[sqlx::test]
async fn an_upload_replaces_only_its_machines_usage_inside_its_window(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let first = upload(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        vec![
            bucket(datetime!(2026-09-22 07:00 UTC), 1, Some(0.1)),
            bucket(datetime!(2026-09-22 08:00 UTC), 2, Some(0.2)),
            bucket(datetime!(2026-09-22 10:00 UTC), 3, Some(0.3)),
        ],
        vec![hint(datetime!(2026-09-22 08:00 UTC))],
    );
    let other_machine = upload(
        MACHINE_B,
        "desktop",
        WINDOW,
        vec![bucket(datetime!(2026-09-22 08:00 UTC), 5, Some(0.5))],
        vec![hint(datetime!(2026-09-22 08:00 UTC))],
    );
    repository.replace_window(&user, &first).await.unwrap();
    repository
        .replace_window(&user, &other_machine)
        .await
        .unwrap();

    let resync = upload(
        MACHINE_A,
        "work-laptop",
        (
            datetime!(2026-09-22 08:00 UTC),
            datetime!(2026-09-22 10:00 UTC),
        ),
        vec![bucket(datetime!(2026-09-22 09:00 UTC), 4, Some(0.4))],
        Vec::new(),
    );
    assert_eq!(repository.replace_window(&user, &resync).await.unwrap(), 1);

    let claude = || "claude".to_string();
    assert_eq!(
        stored_buckets(&pool, MACHINE_A).await,
        [
            (datetime!(2026-09-22 07:00 UTC), claude(), 1),
            (datetime!(2026-09-22 09:00 UTC), claude(), 4),
            (datetime!(2026-09-22 10:00 UTC), claude(), 3),
        ]
    );
    assert_eq!(
        stored_buckets(&pool, MACHINE_B).await,
        [(datetime!(2026-09-22 08:00 UTC), claude(), 5)]
    );
    assert_eq!(
        stored_hints(&pool).await,
        [(MACHINE_B.to_string(), datetime!(2026-09-22 08:00 UTC))]
    );
}

#[sqlx::test]
async fn an_upload_covering_only_codex_keeps_claude_usage_in_its_window(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "work-laptop",
                WINDOW,
                vec![bucket(hour, 2, Some(0.2)), codex_bucket(hour, 3)],
                vec![hint(hour)],
            ),
        )
        .await
        .unwrap();

    let codex_only = upload_covering(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        &[(AiProvider::Codex, AiCoverageStatus::Ok)],
        vec![codex_bucket(datetime!(2026-09-22 09:00 UTC), 4)],
        Vec::new(),
    );
    repository.replace_window(&user, &codex_only).await.unwrap();

    assert_eq!(
        stored_buckets(&pool, MACHINE_A).await,
        [
            (hour, "claude".to_string(), 2),
            (datetime!(2026-09-22 09:00 UTC), "codex".to_string(), 4),
        ]
    );
    assert!(stored_hints(&pool).await.is_empty());
}

#[sqlx::test]
async fn missing_or_failed_history_keeps_stored_usage(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "work-laptop",
                WINDOW,
                vec![bucket(hour, 2, Some(0.2)), codex_bucket(hour, 3)],
                vec![hint(hour)],
            ),
        )
        .await
        .unwrap();
    let stored = stored_buckets(&pool, MACHINE_A).await;

    let unreadable = upload_covering(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        &[
            (AiProvider::Claude, AiCoverageStatus::Missing),
            (AiProvider::Codex, AiCoverageStatus::Failed),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        repository.replace_window(&user, &unreadable).await.unwrap(),
        0
    );

    assert_eq!(stored_buckets(&pool, MACHINE_A).await, stored);
    assert_eq!(stored_hints(&pool).await.len(), 1);
    let statuses: Vec<(String, String)> =
        sqlx::query_as("SELECT provider, status FROM ai_usage_machine_coverage ORDER BY provider")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        statuses,
        [
            ("claude".to_string(), "missing".to_string()),
            ("codex".to_string(), "failed".to_string()),
        ]
    );
}

#[sqlx::test]
async fn later_uploads_update_the_machine_and_the_coverage_they_report(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    repository
        .replace_window(
            &user,
            &upload(MACHINE_A, "old-name", WINDOW, Vec::new(), Vec::new()),
        )
        .await
        .unwrap();

    let later = AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(MACHINE_A).unwrap(),
            label: "new-name".to_string(),
            client_version: "0.3.0".to_string(),
            time_zone: "Asia/Kolkata".to_string(),
        },
        AiUsageWindow::new(
            datetime!(2026-09-23 06:00 UTC),
            datetime!(2026-09-23 12:00 UTC),
        )
        .unwrap(),
        AiPricing {
            status: AiPricingStatus::Unavailable,
            fetched_at: None,
            source: String::new(),
        },
        Vec::new(),
        vec![AiProviderCoverage {
            provider: AiProvider::Codex,
            status: AiCoverageStatus::Failed,
            files: 2,
            unreadable: 1,
            malformed_lines: 1,
            skipped_records: 4,
            duplicates: 0,
        }],
        Vec::new(),
        AiPartialBucketCost::PricedSubtotal,
    )
    .unwrap();
    repository.replace_window(&user, &later).await.unwrap();

    let machine: (String, String, String, bool) = sqlx::query_as(
        "SELECT label, client_version, time_zone, last_synced_at > first_seen_at
         FROM ai_usage_machines",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        machine,
        (
            "new-name".to_string(),
            "0.3.0".to_string(),
            "Asia/Kolkata".to_string(),
            true,
        )
    );

    type CoverageRow = (String, String, i64, i64, i64, i64, OffsetDateTime);
    let coverage: Vec<CoverageRow> = sqlx::query_as(
        "SELECT provider, status, files, unreadable, malformed_lines, skipped_records,
             window_start
         FROM ai_usage_machine_coverage
         ORDER BY provider",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        coverage,
        [
            ("claude".to_string(), "ok".to_string(), 3, 0, 0, 0, WINDOW.0),
            (
                "codex".to_string(),
                "failed".to_string(),
                2,
                1,
                1,
                4,
                datetime!(2026-09-23 06:00 UTC)
            ),
        ]
    );
}

#[sqlx::test]
async fn pricing_is_recorded_only_for_the_providers_it_prices(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    repository
        .replace_window(
            &user,
            &upload(MACHINE_A, "work-laptop", WINDOW, Vec::new(), Vec::new()),
        )
        .await
        .unwrap();

    let mut codex_only = upload_covering(
        MACHINE_A,
        "work-laptop",
        WINDOW,
        &[
            (AiProvider::Codex, AiCoverageStatus::Partial),
            (AiProvider::Claude, AiCoverageStatus::Failed),
        ],
        Vec::new(),
        Vec::new(),
    );
    codex_only = AiUsageUpload::new(
        codex_only.machine().clone(),
        *codex_only.window(),
        AiPricing {
            status: AiPricingStatus::Unavailable,
            fetched_at: None,
            source: String::new(),
        },
        Vec::new(),
        codex_only.coverage().to_vec(),
        Vec::new(),
        AiPartialBucketCost::PricedSubtotal,
    )
    .unwrap();
    repository.replace_window(&user, &codex_only).await.unwrap();

    let pricing: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT provider, pricing_status, pricing_source
         FROM ai_usage_machine_coverage
         ORDER BY provider",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        pricing,
        [
            (
                "claude".to_string(),
                Some("fresh".to_string()),
                Some("https://example.com/prices.json".to_string())
            ),
            (
                "codex".to_string(),
                Some("unavailable".to_string()),
                Some(String::new())
            ),
        ]
    );
}

#[sqlx::test]
async fn uploads_log_what_they_cover_until_they_were_reported(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let every_status = [
        (AiProvider::Claude, AiCoverageStatus::Ok),
        (AiProvider::Codex, AiCoverageStatus::Partial),
        (AiProvider::Grok, AiCoverageStatus::Failed),
        (AiProvider::Copilot, AiCoverageStatus::Missing),
    ];
    let claude_ok = [(AiProvider::Claude, AiCoverageStatus::Ok)];
    // A window in the past, one that ends after the upload, and one that
    // starts after it.
    for (window, coverage) in [
        (WINDOW, &every_status[..]),
        (
            (
                datetime!(2026-09-22 06:00 UTC),
                datetime!(2099-01-01 00:00 UTC),
            ),
            &claude_ok[..],
        ),
        (
            (
                datetime!(2099-01-01 00:00 UTC),
                datetime!(2099-01-02 00:00 UTC),
            ),
            &claude_ok[..],
        ),
    ] {
        let upload = upload_covering(
            MACHINE_A,
            "laptop",
            window,
            coverage,
            Vec::new(),
            Vec::new(),
        );
        repository.replace_window(&user, &upload).await.unwrap();
    }

    type LogRow = (String, OffsetDateTime, OffsetDateTime, OffsetDateTime);
    let log: Vec<LogRow> = sqlx::query_as(
        "SELECT provider, window_start, covered_until, reported_at
         FROM ai_usage_coverage_log
         ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // Only replaced providers are logged; failed and missing prove nothing.
    let logged: Vec<(&str, OffsetDateTime)> = log
        .iter()
        .map(|(provider, start, _, _)| (provider.as_str(), *start))
        .collect();
    assert_eq!(
        logged,
        [
            ("claude", WINDOW.0),
            ("codex", WINDOW.0),
            ("claude", datetime!(2026-09-22 06:00 UTC)),
        ]
    );
    assert_eq!((log[0].2, log[1].2), (WINDOW.1, WINDOW.1));
    // An upload cannot hold usage recorded after it.
    let (_, _, covered_until, reported_at) = log[2];
    assert_eq!(covered_until, reported_at);
    assert!(reported_at < datetime!(2099-01-01 00:00 UTC));
}

#[sqlx::test]
async fn an_upload_that_read_history_earlier_cannot_replace_a_later_one(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let read_at = |snapshot_at, records| {
        upload(
            MACHINE_A,
            "work-laptop",
            WINDOW,
            vec![bucket(datetime!(2026-09-22 08:00 UTC), records, Some(0.5))],
            Vec::new(),
        )
        .with_snapshot_at(snapshot_at)
    };
    let later = read_at(Some(datetime!(2026-09-22 10:30 UTC)), 5);
    repository.replace_window(&user, &later).await.unwrap();
    let stored = snapshot(&pool).await;
    let logged = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ai_usage_coverage_log")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let logged_before = logged().await;

    // A slow upload or a retry of an earlier run arrives after the later one.
    let earlier = read_at(Some(datetime!(2026-09-22 10:00 UTC)), 3);
    assert!(matches!(
        repository.replace_window(&user, &earlier).await,
        Err(AiUsageError::SupersededUpload)
    ));
    assert_eq!(snapshot(&pool).await, stored);
    assert_eq!(logged().await, logged_before);

    // Retrying the stored upload stays harmless.
    repository.replace_window(&user, &later).await.unwrap();
    assert_eq!(snapshot(&pool).await, stored);

    // Clients that do not report a snapshot keep replacing their window.
    repository
        .replace_window(&user, &read_at(None, 3))
        .await
        .unwrap();
    assert_eq!(
        stored_buckets(&pool, MACHINE_A).await,
        [(datetime!(2026-09-22 08:00 UTC), "claude".to_string(), 3)]
    );
}

#[sqlx::test]
async fn a_snapshot_ahead_of_the_server_clock_does_not_block_later_uploads(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let fast_clock = upload(MACHINE_A, "laptop", WINDOW, Vec::new(), Vec::new())
        .with_snapshot_at(Some(datetime!(2099-01-01 00:00 UTC)));

    repository.replace_window(&user, &fast_clock).await.unwrap();
    repository.replace_window(&user, &fast_clock).await.unwrap();
    let corrected = upload(MACHINE_A, "laptop", WINDOW, Vec::new(), Vec::new())
        .with_snapshot_at(Some(OffsetDateTime::now_utc() + time::Duration::minutes(1)));
    repository.replace_window(&user, &corrected).await.unwrap();
}

#[sqlx::test]
async fn snapshots_from_a_slightly_fast_clock_keep_their_order(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let ahead = |minutes| {
        upload(MACHINE_A, "laptop", WINDOW, Vec::new(), Vec::new()).with_snapshot_at(Some(
            OffsetDateTime::now_utc() + time::Duration::minutes(minutes),
        ))
    };

    // Both read history "in the future"; the later read commits first.
    repository.replace_window(&user, &ahead(2)).await.unwrap();
    assert!(matches!(
        repository.replace_window(&user, &ahead(1)).await,
        Err(AiUsageError::SupersededUpload)
    ));
}

#[sqlx::test]
async fn an_upload_proves_usage_complete_only_until_its_snapshot(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let upload = upload(MACHINE_A, "laptop", WINDOW, Vec::new(), Vec::new())
        .with_snapshot_at(Some(datetime!(2026-09-22 09:15 UTC)));
    repository.replace_window(&user, &upload).await.unwrap();

    let covered: Vec<(String, OffsetDateTime)> = sqlx::query_as(
        "SELECT provider, covered_until FROM ai_usage_coverage_log ORDER BY provider",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        covered,
        [
            ("claude".to_string(), datetime!(2026-09-22 09:15 UTC)),
            ("codex".to_string(), datetime!(2026-09-22 09:15 UTC)),
        ]
    );
}

#[sqlx::test]
async fn the_longest_allowed_text_is_stored(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    // 512 varied four-byte characters: the largest keys a valid upload can
    // carry, and too irregular for Postgres to compress.
    let longest = (0..512u32)
        .map(|index| char::from_u32(0x10000 + index.wrapping_mul(2_654_435_761) % 0xF_0000))
        .collect::<Option<String>>()
        .unwrap();
    let hour = datetime!(2026-09-22 08:00 UTC);
    let mut long_bucket = bucket(hour, 1, Some(0.1));
    long_bucket.project_key = longest.clone();
    long_bucket.model = longest.clone();
    let long_hint = AiProviderHint {
        plan: longest.clone(),
        ..hint(hour)
    };

    let stored = repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                &longest,
                WINDOW,
                vec![long_bucket],
                vec![long_hint],
            ),
        )
        .await
        .unwrap();

    assert_eq!(stored, 1);
}

#[sqlx::test]
async fn only_time_zones_the_database_knows_are_accepted(pool: PgPool) {
    let repository = repository(&pool);
    let knows = |name: &'static str| {
        let repository = &repository;
        async move {
            repository
                .knows_time_zone(&AiUsageTimeZone::parse(name).unwrap())
                .await
                .unwrap()
        }
    };

    assert!(knows("Europe/Stockholm").await);
    assert!(!knows("Europe/Stokholm").await);
}

fn basis(raw: &str) -> AiPricingBasis {
    AiPricingBasis::parse(raw).unwrap()
}

fn machine(raw: &str) -> AiMachineId {
    AiMachineId::parse(raw).unwrap()
}

/// Stored `(session_key, billing_provider, cost, unpriced_records,
/// pricing_basis)` of every bucket, by session and payer.
type StoredCost = (String, Option<String>, Option<f64>, i64, Option<String>);

async fn stored_costs(pool: &PgPool) -> Vec<StoredCost> {
    sqlx::query_as(
        "SELECT session_key, billing_provider, estimated_cost_usd::float8, unpriced_records,
             pricing_basis
         FROM ai_usage_buckets
         ORDER BY machine_id, hour_start, session_key, billing_provider NULLS FIRST",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Every stored row other than buckets, sync times included, without the
/// machine's current pricing basis.
async fn sync_state(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT row_to_json(h)::text FROM (
             SELECT * FROM ai_usage_provider_hints ORDER BY machine_id, hour_start, plan
         ) h
         UNION ALL
         SELECT row_to_json(c)::text FROM (
             SELECT * FROM ai_usage_machine_coverage ORDER BY machine_id, provider
         ) c
         UNION ALL
         SELECT row_to_json(l)::text FROM (SELECT * FROM ai_usage_coverage_log ORDER BY id) l
         UNION ALL
         SELECT (to_jsonb(m) - 'pricing_basis')::text FROM (
             SELECT * FROM ai_usage_machines ORDER BY id
         ) m",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn session(index: u8) -> String {
    format!("{index:032x}")
}

#[sqlx::test]
async fn an_upload_stores_its_pricing_basis_on_every_bucket(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let buckets = || vec![bucket(hour, 2, Some(0.2)), codex_bucket(hour, 3)];
    repository
        .replace_window(
            &user,
            &upload(MACHINE_A, "laptop", WINDOW, buckets(), Vec::new())
                .with_pricing_basis(Some(basis("copilot-rates@2026-10-24"))),
        )
        .await
        .unwrap();
    repository
        .replace_window(
            &user,
            &upload(MACHINE_B, "desktop", WINDOW, buckets(), Vec::new()),
        )
        .await
        .unwrap();

    let bases: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT machine_id::text, provider, pricing_basis FROM ai_usage_buckets
         ORDER BY machine_id, provider",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let new = || Some("copilot-rates@2026-10-24".to_string());
    assert_eq!(
        bases,
        [
            (MACHINE_B.to_string(), "claude".to_string(), None),
            (MACHINE_B.to_string(), "codex".to_string(), None),
            (MACHINE_A.to_string(), "claude".to_string(), new()),
            (MACHINE_A.to_string(), "codex".to_string(), new()),
        ]
    );
}

/// The current pricing basis of `machine_id`.
async fn machine_basis(pool: &PgPool, machine_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT pricing_basis FROM ai_usage_machines WHERE id = $1::uuid")
        .bind(machine_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test]
async fn the_latest_basis_uploaded_or_repriced_is_the_machines_pricing_basis(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let intruder = insert_user(&pool, "intruder@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let synced = |pricing_basis: Option<&str>, snapshot_at| {
        upload(
            MACHINE_A,
            "laptop",
            WINDOW,
            vec![bucket(hour, 2, Some(0.2))],
            Vec::new(),
        )
        .with_pricing_basis(pricing_basis.map(basis))
        .with_snapshot_at(snapshot_at)
    };

    repository
        .replace_window(
            &user,
            &synced(Some("v1"), Some(datetime!(2026-09-22 10:00 UTC))),
        )
        .await
        .unwrap();
    assert_eq!(machine_basis(&pool, MACHINE_A).await.as_deref(), Some("v1"));

    // A repricing sets its basis even when no bucket matches.
    let unmatched = bucket(datetime!(2026-09-22 09:00 UTC), 1, Some(0.1));
    let updated = repository
        .reprice_buckets(&user, &machine(MACHINE_A), &basis("v2"), &[&unmatched])
        .await
        .unwrap();
    assert_eq!(updated, 0);
    assert_eq!(machine_basis(&pool, MACHINE_A).await.as_deref(), Some("v2"));

    // Neither another user's repricing nor a refused upload changes it.
    repository
        .reprice_buckets(&intruder, &machine(MACHINE_A), &basis("v3"), &[])
        .await
        .unwrap();
    assert!(matches!(
        repository
            .replace_window(
                &user,
                &synced(Some("v3"), Some(datetime!(2026-09-22 09:00 UTC))),
            )
            .await,
        Err(AiUsageError::SupersededUpload)
    ));
    assert_eq!(machine_basis(&pool, MACHINE_A).await.as_deref(), Some("v2"));

    // An upload without a basis, as from a run priced without vendor rates,
    // keeps the machine's.
    repository
        .replace_window(&user, &synced(None, None))
        .await
        .unwrap();
    assert_eq!(machine_basis(&pool, MACHINE_A).await.as_deref(), Some("v2"));
}

#[sqlx::test]
async fn a_repricing_updates_only_buckets_it_reproduces_exactly(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let stored = |index: u8, payer: Option<AiProvider>| AiUsageBucket {
        session_key: session(index),
        billing_provider: payer,
        ..bucket(hour, 2, Some(0.2))
    };
    let codex = Some(AiProvider::Codex);
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "laptop",
                WINDOW,
                vec![
                    stored(1, None),
                    stored(2, codex),
                    stored(3, None),
                    stored(4, None),
                    stored(5, None),
                    stored(6, codex),
                    stored(7, None),
                ],
                vec![hint(hour)],
            ),
        )
        .await
        .unwrap();
    let state = sync_state(&pool).await;

    let repriced = |index: u8, payer: Option<AiProvider>| AiUsageBucket {
        estimated_cost_usd: Some(0.5),
        unpriced_records: 1,
        ..stored(index, payer)
    };
    let mut more_output = repriced(3, None);
    more_output.tokens.output += 1;
    let mut more_records = repriced(4, None);
    more_records.records += 1;
    let mut other_model = repriced(7, None);
    other_model.model = "other-model".to_string();
    let never_stored = AiUsageBucket {
        hour_start: datetime!(2026-09-22 09:00 UTC),
        ..repriced(1, None)
    };
    let buckets = [
        repriced(1, None),
        repriced(2, codex),
        more_output,
        more_records,
        // The payer is part of the key, NULL included.
        repriced(5, codex),
        repriced(6, None),
        other_model,
        never_stored,
    ];

    let updated = repository
        .reprice_buckets(
            &user,
            &machine(MACHINE_A),
            &basis("new"),
            &buckets.iter().collect::<Vec<_>>(),
        )
        .await
        .unwrap();

    assert_eq!(updated, 2);
    let old = |index: u8, payer: Option<&str>| {
        (
            session(index),
            payer.map(str::to_string),
            Some(0.2),
            0,
            None,
        )
    };
    assert_eq!(
        stored_costs(&pool).await,
        [
            (session(1), None, Some(0.5), 1, Some("new".to_string())),
            (
                session(2),
                Some("codex".to_string()),
                Some(0.5),
                1,
                Some("new".to_string())
            ),
            old(3, None),
            old(4, None),
            old(5, None),
            old(6, Some("codex")),
            old(7, None),
        ]
    );
    assert_eq!(sync_state(&pool).await, state);

    // Repricing again with the same values still counts every match.
    let updated = repository
        .reprice_buckets(
            &user,
            &machine(MACHINE_A),
            &basis("new"),
            &buckets.iter().collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    assert_eq!(updated, 2);
}

#[sqlx::test]
async fn repricing_sees_only_the_users_own_machine(pool: PgPool) {
    let repository = repository(&pool);
    let owner = insert_user(&pool, "owner@example.com").await;
    let intruder = insert_user(&pool, "intruder@example.com").await;
    let stored = bucket(datetime!(2026-09-22 08:00 UTC), 2, Some(0.2));
    repository
        .replace_window(
            &owner,
            &upload(
                MACHINE_A,
                "laptop",
                WINDOW,
                vec![stored.clone()],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    let floor = datetime!(2026-09-01 00:00 UTC);

    for machine_id in [MACHINE_A, MACHINE_B] {
        let status = repository
            .stale_pricing(&intruder, &machine(machine_id), &basis("new"), floor)
            .await
            .unwrap();
        assert_eq!(status, [], "{machine_id}");
        let updated = repository
            .reprice_buckets(&intruder, &machine(machine_id), &basis("new"), &[&stored])
            .await
            .unwrap();
        assert_eq!(updated, 0, "{machine_id}");
    }

    assert_eq!(snapshot(&pool).await, before);
}

#[sqlx::test]
async fn stale_pricing_finds_the_earliest_hour_another_basis_priced(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let at = |hour: u8| datetime!(2026-09-22 00:00 UTC).replace_hour(hour).unwrap();
    let grok_bucket = |hour| AiUsageBucket {
        provider: AiProvider::Grok,
        ..bucket(hour, 1, Some(0.1))
    };
    let only = |provider| [(provider, AiCoverageStatus::Ok)];
    // No basis until 09:00, an old one from then on, and the current one for
    // Codex from 10:00 and for Grok.
    for (machine_id, window, coverage, buckets, pricing_basis) in [
        (
            MACHINE_A,
            (at(6), at(9)),
            BOTH_OK,
            vec![bucket(at(7), 1, Some(0.1)), bucket(at(8), 1, Some(0.1))],
            None,
        ),
        (
            MACHINE_A,
            (at(9), at(12)),
            BOTH_OK,
            vec![bucket(at(9), 1, Some(0.1)), codex_bucket(at(9), 1)],
            Some("old"),
        ),
        (
            MACHINE_A,
            (at(10), at(12)),
            &only(AiProvider::Codex)[..],
            vec![codex_bucket(at(10), 1), codex_bucket(at(11), 1)],
            Some("new"),
        ),
        (
            MACHINE_A,
            (at(6), at(12)),
            &only(AiProvider::Grok)[..],
            vec![grok_bucket(at(10))],
            Some("new"),
        ),
        (
            MACHINE_B,
            (at(6), at(12)),
            BOTH_OK,
            vec![bucket(at(7), 1, Some(0.1)), codex_bucket(at(10), 1)],
            None,
        ),
    ] {
        let upload = upload_covering(machine_id, "laptop", window, coverage, buckets, Vec::new())
            .with_pricing_basis(pricing_basis.map(basis));
        repository.replace_window(&user, &upload).await.unwrap();
    }

    let status = repository
        .stale_pricing(&user, &machine(MACHINE_A), &basis("new"), at(8))
        .await
        .unwrap();

    assert_eq!(
        status,
        [
            AiProviderRepricing {
                provider: AiProvider::Claude,
                from: Some(at(8)),
                stale_buckets: 2,
            },
            AiProviderRepricing {
                provider: AiProvider::Codex,
                from: Some(at(9)),
                stale_buckets: 1,
            },
            AiProviderRepricing {
                provider: AiProvider::Grok,
                from: None,
                stale_buckets: 0,
            },
        ]
    );
}
