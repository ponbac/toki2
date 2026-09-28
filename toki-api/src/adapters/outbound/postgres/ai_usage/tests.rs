use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    OffsetDateTime,
};

use crate::{
    adapters::outbound::postgres::PostgresAiProjectMappingRepository,
    domain::{
        models::{
            AiCoverageStatus, AiMachine, AiMachineId, AiMappedProject, AiPricing, AiPricingStatus,
            AiProjectKey, AiProvider, AiProviderCoverage, AiProviderHint, AiUsageBucket,
            AiUsageWindow, ProjectId, UNATTRIBUTED_PROJECT_KEY,
        },
        ports::outbound::AiProjectMappingRepository,
    },
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
    PostgresAiUsageRepository::new(sqlx_tracing::PoolBuilder::from(pool.clone()).build())
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
async fn totals_sum_machines_and_keep_unknown_costs_visible(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    let other_user = insert_user(&pool, "other@example.com").await;
    let window = (
        datetime!(2026-09-22 06:00 UTC),
        datetime!(2026-09-22 12:00 UTC),
    );
    let hour = datetime!(2026-09-22 08:00 UTC);
    let mut free = bucket(hour, 4, Some(0.0));
    free.project_key = "Client A".to_string();

    for (machine_id, owner, buckets) in [
        (MACHINE_A, user, vec![bucket(hour, 1, Some(1.25)), free]),
        (MACHINE_B, user, vec![bucket(hour, 2, None)]),
        (
            "7e57c0de-0000-4000-8000-000000000001",
            other_user,
            vec![bucket(hour, 8, Some(9.0))],
        ),
    ] {
        repository
            .replace_window(
                &owner,
                &upload(machine_id, "laptop", window, buckets, Vec::new()),
            )
            .await
            .unwrap();
    }

    let totals = repository
        .period_totals(
            &user,
            AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap(),
            AiUsagePeriod::Day,
            &AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
            None,
        )
        .await
        .unwrap();

    assert_eq!(
        totals,
        [
            AiUsagePeriodTotals {
                dates: AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap(),
                project_key: "Client A".to_string(),
                project: None,
                totals: AiUsageTotals {
                    tokens: bucket(hour, 4, None).tokens,
                    records: 4,
                    priced_cost_usd: 0.0,
                    unpriced_buckets: 0,
                    unpriced_records: 0,
                },
            },
            AiUsagePeriodTotals {
                dates: AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap(),
                project_key: PROJECT.to_string(),
                project: None,
                totals: AiUsageTotals {
                    tokens: bucket(hour, 3, None).tokens,
                    records: 3,
                    priced_cost_usd: 1.25,
                    unpriced_buckets: 1,
                    unpriced_records: 2,
                },
            },
        ]
    );
    assert_eq!(totals[0].totals.estimated_cost_usd(), Some(0.0));
    assert_eq!(totals[1].totals.estimated_cost_usd(), None);
}

#[sqlx::test]
async fn totals_report_each_keys_current_mapping_when_they_are_read(pool: PgPool) {
    let repository = repository(&pool);
    let mappings = PostgresAiProjectMappingRepository::new(
        sqlx_tracing::PoolBuilder::from(pool.clone()).build(),
    );
    let user = insert_user(&pool, "dev@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let mut unattributed = bucket(hour, 2, Some(0.2));
    unattributed.project_key = UNATTRIBUTED_PROJECT_KEY.to_string();
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "laptop",
                WINDOW,
                vec![bucket(hour, 1, Some(0.1)), unattributed],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    let company = |company_id: &str| TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: company_id.to_string(),
    };
    let configured = company("company-1");
    let key = AiProjectKey::parse(PROJECT).unwrap();
    let project = |id: &str, name: &str| AiMappedProject {
        id: ProjectId::new(id),
        name: name.to_string(),
    };
    let mapped_projects = || async {
        repository
            .period_totals(
                &user,
                AiUsageDateRange::new(date!(2026 - 09 - 01), date!(2026 - 10 - 01)).unwrap(),
                AiUsagePeriod::Month,
                &AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
                Some(&configured),
            )
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.project_key, row.project, row.totals.records))
            .collect::<Vec<_>>()
    };
    let unassigned = (UNATTRIBUTED_PROJECT_KEY.to_string(), None, 2);

    assert_eq!(
        mapped_projects().await,
        [(PROJECT.to_string(), None, 1), unassigned.clone()]
    );

    // Mapping, remapping and unmapping apply to usage stored before them. A
    // mapping to another company's project is stale and resolves to nothing.
    for (saved, resolved) in [
        (
            Some((configured.clone(), project("101", "Client A delivery"))),
            Some(project("101", "Client A delivery")),
        ),
        (
            Some((configured.clone(), project("202", "Internal tools"))),
            Some(project("202", "Internal tools")),
        ),
        (
            Some((company("company-2"), project("303", "Elsewhere"))),
            None,
        ),
        (None, None),
    ] {
        match saved {
            Some((company, project)) => {
                mappings
                    .upsert(&key, &company, &project, &user)
                    .await
                    .unwrap();
            }
            None => assert!(mappings.delete(&key).await.unwrap()),
        }
        assert_eq!(
            mapped_projects().await,
            [(PROJECT.to_string(), resolved, 1), unassigned.clone()]
        );
    }
}

#[sqlx::test]
async fn stockholm_months_follow_daylight_saving_time(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "dev@example.com").await;
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "laptop",
                (
                    datetime!(2026-09-30 00:00 UTC),
                    datetime!(2026-11-01 00:00 UTC),
                ),
                vec![
                    // 23:00 on 30 September, summer time (UTC+2).
                    bucket(datetime!(2026-09-30 21:00 UTC), 1, Some(0.1)),
                    // Midnight on 1 October, summer time.
                    bucket(datetime!(2026-09-30 22:00 UTC), 10, Some(0.1)),
                    // 23:00 on 31 October, standard time (UTC+1).
                    bucket(datetime!(2026-10-31 22:00 UTC), 100, Some(0.1)),
                    // Midnight on 1 November, standard time.
                    bucket(datetime!(2026-10-31 23:00 UTC), 1000, Some(0.1)),
                ],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
    let monthly_records = |start, end| {
        let repository = &repository;
        let stockholm = &stockholm;
        async move {
            repository
                .period_totals(
                    &user,
                    AiUsageDateRange::new(start, end).unwrap(),
                    AiUsagePeriod::Month,
                    stockholm,
                    None,
                )
                .await
                .unwrap()
                .into_iter()
                .map(|row| (row.dates.start(), row.dates.end(), row.totals.records))
                .collect::<Vec<_>>()
        }
    };

    assert_eq!(
        monthly_records(date!(2026 - 09 - 01), date!(2026 - 12 - 01)).await,
        [
            (date!(2026 - 09 - 01), date!(2026 - 10 - 01), 1),
            (date!(2026 - 10 - 01), date!(2026 - 11 - 01), 110),
            (date!(2026 - 11 - 01), date!(2026 - 12 - 01), 1000),
        ]
    );
    assert_eq!(
        monthly_records(date!(2026 - 10 - 01), date!(2026 - 11 - 01)).await,
        [(date!(2026 - 10 - 01), date!(2026 - 11 - 01), 110)]
    );
    // A range that covers months only partly reports the dates it covered.
    assert_eq!(
        monthly_records(date!(2026 - 09 - 30), date!(2026 - 10 - 02)).await,
        [
            (date!(2026 - 09 - 30), date!(2026 - 10 - 01), 1),
            (date!(2026 - 10 - 01), date!(2026 - 10 - 02), 10),
        ]
    );
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

#[sqlx::test]
async fn period_totals_rejects_integer_precision_loss(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "numeric@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let mut first = bucket(hour, 1, Some(0.01));
    first.tokens = AiTokenCounts {
        input: crate::domain::models::MAX_SAFE_COUNT,
        ..AiTokenCounts::default()
    };
    let mut second = first.clone();
    second.session_key = "00000000000000000000000000000001".to_string();
    second.tokens.input = 1;
    let dates = AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap();
    let zone = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "fixture",
                WINDOW,
                vec![first.clone()],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    let totals = repository
        .period_totals(&user, dates, AiUsagePeriod::Day, &zone, None)
        .await
        .unwrap();
    assert_eq!(
        totals[0].totals.tokens.input,
        crate::domain::models::MAX_SAFE_COUNT
    );
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "fixture",
                WINDOW,
                vec![first, second],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    let error = repository
        .period_totals(&user, dates, AiUsagePeriod::Day, &zone, None)
        .await
        .unwrap_err();
    assert!(matches!(error, AiUsageError::NumericRange));
    let response =
        axum::response::IntoResponse::into_response(crate::routes::ApiError::from(error));
    assert_eq!(
        response.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let body = axum::body::to_bytes(response.into_body(), 1_024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"],
        "ai usage result exceeds the supported numeric range"
    );
}

#[sqlx::test]
async fn period_totals_rejects_database_integer_overflow(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "numeric@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let buckets = (0..1_025)
        .map(|index| {
            let mut value = bucket(hour, 1, Some(0.01));
            value.session_key = format!("{index:032x}");
            value.tokens = AiTokenCounts {
                input: crate::domain::models::MAX_SAFE_COUNT,
                ..AiTokenCounts::default()
            };
            value
        })
        .collect();
    repository
        .replace_window(
            &user,
            &upload(MACHINE_A, "fixture", WINDOW, buckets, Vec::new()),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .period_totals(
                &user,
                AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap(),
                AiUsagePeriod::Day,
                &AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
                None,
            )
            .await
            .unwrap_err()
            .to_string(),
        "ai usage result exceeds the supported numeric range"
    );
}

#[sqlx::test]
async fn period_totals_rejects_database_cost_overflow(pool: PgPool) {
    let repository = repository(&pool);
    let user = insert_user(&pool, "numeric@example.com").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let first = bucket(hour, 1, Some(1e308));
    let mut second = first.clone();
    second.session_key = "00000000000000000000000000000001".to_string();
    let dates = AiUsageDateRange::new(date!(2026 - 09 - 22), date!(2026 - 09 - 23)).unwrap();
    let zone = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "fixture",
                WINDOW,
                vec![first.clone()],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .period_totals(&user, dates, AiUsagePeriod::Day, &zone, None)
            .await
            .unwrap()[0]
            .totals
            .priced_cost_usd,
        1e308
    );
    repository
        .replace_window(
            &user,
            &upload(
                MACHINE_A,
                "fixture",
                WINDOW,
                vec![first, second],
                Vec::new(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .period_totals(&user, dates, AiUsagePeriod::Day, &zone, None)
            .await
            .unwrap_err()
            .to_string(),
        "ai usage result exceeds the supported numeric range"
    );
}
