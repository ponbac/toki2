use std::sync::Arc;

use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    Duration, OffsetDateTime,
};

use super::*;
use crate::{
    adapters::outbound::postgres::{PostgresAiProjectMappingRepository, PostgresAiUsageRepository},
    domain::{
        models::{
            AiMappedProject, AiProjectFilter, AiProjectKey, AiProjectResolution, AiUsageBucket,
            AiUsageFilter, AiUsagePeriod, AiUsageReportQuery, AiUsageUpload, AiUsageWindow,
            TimeTrackingCompany, UNATTRIBUTED_PROJECT_KEY,
        },
        ports::{
            inbound::AiUsageReportService,
            outbound::{AiProjectMappingRepository, AiUsageRepository},
        },
        services::AiUsageReportServiceImpl,
    },
};

pub(crate) const MACHINE_A: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
pub(crate) const MACHINE_B: &str = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
pub(crate) const APP: &str = "github.com/example/app";
const SESSION: &str = "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f";

fn db(pool: &PgPool) -> DbPool {
    sqlx_tracing::PoolBuilder::from(pool.clone()).build()
}

fn stockholm() -> AiUsageTimeZone {
    AiUsageTimeZone::parse("Europe/Stockholm").unwrap()
}

fn company(id: &str) -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: id.to_string(),
    }
}

/// The report service over the store, with time tracking configured for
/// `company-1` or not configured at all.
fn service(
    pool: &PgPool,
    configured: bool,
) -> AiUsageReportServiceImpl<PostgresAiUsageReportRepository> {
    AiUsageReportServiceImpl::new(
        Arc::new(PostgresAiUsageReportRepository::new(db(pool))),
        stockholm(),
        configured.then(|| company("company-1")),
    )
}

fn query(from: Date, to: Date, filter: AiUsageFilter) -> AiUsageReportQuery {
    AiUsageReportQuery {
        from: Some(from),
        to: Some(to),
        filter,
    }
}

fn september(filter: AiUsageFilter) -> AiUsageReportQuery {
    query(date!(2026 - 09 - 01), date!(2026 - 09 - 30), filter)
}

#[sqlx::test]
async fn personal_reads_reject_integer_precision_loss(pool: PgPool) {
    let user = insert_user(&pool, "numeric@example.com", "User").await;
    let mut value = bucket(datetime!(2026-09-22 08:00 UTC), APP, 1, Some(0.01));
    value.tokens = AiTokenCounts {
        input: 9_007_199_254_740_991,
        ..AiTokenCounts::default()
    };
    record(&pool, user, MACHINE_A, vec![value.clone()]).await;
    let report_service = service(&pool, false);
    let query = september(AiUsageFilter::default());
    assert_eq!(
        report_service
            .report(&user, &query)
            .await
            .unwrap()
            .totals
            .tokens
            .input,
        value.tokens.input
    );
    value.tokens.output = 1;
    record(&pool, user, MACHINE_A, vec![value]).await;
    let error = "ai usage result exceeds the supported numeric range";
    assert_eq!(
        report_service
            .report(&user, &query)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
    assert_eq!(
        report_service
            .sessions(&user, &query, AiSessionOrder::Tokens, None, None)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
    assert_eq!(
        report_service
            .machines(&user, &query)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
}

#[sqlx::test]
async fn personal_reads_reject_database_numeric_overflow(pool: PgPool) {
    let user = insert_user(&pool, "numeric@example.com", "User").await;
    let first = bucket(datetime!(2026-09-22 08:00 UTC), APP, 1, Some(1e308));
    let mut second = first.clone();
    second.hour_start = datetime!(2026-09-22 09:00 UTC);
    record(&pool, user, MACHINE_A, vec![first.clone()]).await;
    let report_service = service(&pool, false);
    let query = september(AiUsageFilter::default());
    assert_eq!(
        report_service
            .report(&user, &query)
            .await
            .unwrap()
            .totals
            .priced_cost_usd,
        1e308
    );
    record(&pool, user, MACHINE_A, vec![first, second]).await;
    let error = "ai usage result exceeds the supported numeric range";
    assert_eq!(
        report_service
            .report(&user, &query)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
    assert_eq!(
        report_service
            .sessions(&user, &query, AiSessionOrder::Cost, None, None)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
    assert_eq!(
        report_service
            .machines(&user, &query)
            .await
            .unwrap_err()
            .to_string(),
        error
    );
}

pub(crate) async fn insert_user(pool: &PgPool, email: &str, role: &str) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
             VALUES ($1, 'Test User', '', '', ARRAY[$2])
             RETURNING id",
    )
    .bind(email)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

/// A synthetic bucket of Claude usage; `None` is an unpriced cost.
pub(crate) fn bucket(
    hour_start: OffsetDateTime,
    project_key: &str,
    records: i64,
    cost: Option<f64>,
) -> AiUsageBucket {
    AiUsageBucket {
        hour_start,
        session_key: SESSION.to_string(),
        project_key: project_key.to_string(),
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

/// A bucket of another session, provider or model.
fn variant(
    bucket: AiUsageBucket,
    session: &str,
    provider: AiProvider,
    model: &str,
) -> AiUsageBucket {
    AiUsageBucket {
        session_key: format!("{session:0>32}"),
        provider,
        model: model.to_string(),
        ..bucket
    }
}

/// Stores buckets from a machine, covering Claude and Codex as ok and Grok
/// as missing, over a window around them.
pub(crate) async fn record(
    pool: &PgPool,
    user: UserId,
    machine_id: &str,
    buckets: Vec<AiUsageBucket>,
) {
    let coverage = |provider, status, unreadable| AiProviderCoverage {
        provider,
        status,
        files: 3,
        unreadable,
        malformed_lines: 0,
        skipped_records: 0,
        duplicates: 0,
    };
    let upload = AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse(machine_id).unwrap(),
            label: format!("laptop-{}", &machine_id[..4]),
            client_version: "0.2.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        AiUsageWindow::new(
            datetime!(2026-03-01 00:00 UTC),
            datetime!(2026-12-01 00:00 UTC),
        )
        .unwrap(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: None,
            source: "https://example.com/prices.json".to_string(),
        },
        buckets,
        vec![
            coverage(AiProvider::Claude, AiCoverageStatus::Ok, 0),
            coverage(AiProvider::Codex, AiCoverageStatus::Ok, 0),
            coverage(AiProvider::Grok, AiCoverageStatus::Missing, 1),
        ],
        Vec::new(),
    )
    .unwrap();
    PostgresAiUsageRepository::new(db(pool))
        .replace_window(&user, &upload)
        .await
        .unwrap();
}

/// Maps a key to a project of a company, as an admin would.
async fn map(pool: &PgPool, by: UserId, key: &str, company_id: &str, project_id: &str) {
    PostgresAiProjectMappingRepository::new(db(pool))
        .upsert(
            &AiProjectKey::parse(key).unwrap(),
            &company(company_id),
            &AiMappedProject {
                id: ProjectId::new(project_id),
                name: format!("Project {project_id}"),
            },
            &by,
        )
        .await
        .unwrap();
}

/// Moves a machine's last sync back to age it.
pub(crate) async fn set_last_synced_at(pool: &PgPool, machine_id: &str, at: OffsetDateTime) {
    sqlx::query("UPDATE ai_usage_machines SET last_synced_at = $2 WHERE id = $1::uuid")
        .bind(machine_id)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
async fn local_days_and_hours_follow_daylight_saving_and_month_ends(pool: PgPool) {
    let user = insert_user(&pool, "dev@example.com", "User").await;
    record(
        &pool,
        user,
        MACHINE_A,
        vec![
            // 23:00 on Wednesday 30 September, summer time (UTC+2).
            bucket(datetime!(2026-09-30 21:00 UTC), APP, 1, Some(0.5)),
            // Midnight on Thursday 1 October, summer time.
            bucket(datetime!(2026-09-30 22:00 UTC), APP, 2, Some(0.5)),
            // Clocks go back on Sunday 25 October: 02:00 happens twice.
            bucket(datetime!(2026-10-25 00:00 UTC), APP, 4, Some(0.25)),
            bucket(datetime!(2026-10-25 01:00 UTC), APP, 8, Some(0.25)),
            // 03:00 on 25 October, standard time (UTC+1).
            bucket(datetime!(2026-10-25 02:00 UTC), APP, 16, None),
            // 23:00 on Saturday 31 October, standard time.
            bucket(datetime!(2026-10-31 22:00 UTC), APP, 32, Some(1.0)),
            // Midnight on Sunday 1 November, standard time.
            bucket(datetime!(2026-10-31 23:00 UTC), APP, 64, Some(1.0)),
            // Clocks went forward on Sunday 29 March: 01:00 UTC is 03:00.
            bucket(datetime!(2026-03-29 01:00 UTC), APP, 128, Some(1.0)),
        ],
    )
    .await;
    let service = service(&pool, true);
    let report = |from, to| {
        let service = &service;
        async move {
            service
                .report(&user, &query(from, to, AiUsageFilter::default()))
                .await
                .unwrap()
        }
    };

    let october = report(date!(2026 - 10 - 01), date!(2026 - 10 - 31)).await;
    assert_eq!(
        october
            .days
            .iter()
            .map(|day| (day.date, day.totals.records))
            .collect::<Vec<_>>(),
        [
            (date!(2026 - 10 - 01), 2),
            (date!(2026 - 10 - 25), 28),
            (date!(2026 - 10 - 31), 32),
        ]
    );
    assert_eq!(
        october
            .hours
            .iter()
            .map(|hour| (hour.weekday, hour.hour, hour.totals.records))
            .collect::<Vec<_>>(),
        [
            (Weekday::Thursday, 0, 2),
            (Weekday::Saturday, 23, 32),
            (Weekday::Sunday, 2, 12),
            (Weekday::Sunday, 3, 16),
        ]
    );
    assert_eq!(october.totals.records, 62);
    assert_eq!(october.totals.priced_cost_usd, 2.0);
    assert_eq!(october.totals.unpriced_records, 16);
    assert_eq!(october.totals.estimated_cost_usd(), None);

    let march = report(date!(2026 - 03 - 29), date!(2026 - 03 - 29)).await;
    assert_eq!(
        march
            .hours
            .iter()
            .map(|hour| (hour.weekday, hour.hour, hour.totals.records))
            .collect::<Vec<_>>(),
        [(Weekday::Sunday, 3, 128)]
    );
}

#[sqlx::test]
async fn costs_are_summed_exactly_like_the_period_totals(pool: PgPool) {
    let user = insert_user(&pool, "dev@example.com", "User").await;
    let other = "github.com/example/other";
    // Ten hours of 0.1 on one key and 0.2 on another. Neither is a binary
    // fraction, so floating-point sums of sums drift from the exact 3.
    let hours = (8..18).map(|hour| datetime!(2026-09-22 00:00 UTC).replace_hour(hour).unwrap());
    record(
        &pool,
        user,
        MACHINE_A,
        hours
            .flat_map(|hour| {
                [
                    bucket(hour, APP, 1, Some(0.1)),
                    bucket(hour, other, 1, Some(0.2)),
                ]
            })
            .collect(),
    )
    .await;
    map(&pool, user, APP, "company-1", "101").await;
    map(&pool, user, other, "company-1", "101").await;
    let drifting = (0..10).fold(0.0, |sum, _| sum + 0.1 + 0.2);
    assert_ne!(drifting, 3.0);

    let report = service(&pool, true)
        .report(&user, &september(AiUsageFilter::default()))
        .await
        .unwrap();

    assert_eq!(report.totals.priced_cost_usd, 3.0);
    assert_eq!(report.days[0].totals.priced_cost_usd, 3.0);
    assert_eq!(report.projects[0].totals.priced_cost_usd, 3.0);
    assert!(report
        .hours
        .iter()
        .all(|hour| hour.totals.priced_cost_usd == 0.3));
    assert_eq!(report.machines[0].totals.priced_cost_usd, 3.0);

    // Per key, the figures are the team's period totals exactly.
    let period_totals = PostgresAiUsageRepository::new(db(&pool))
        .period_totals(
            &user,
            AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), date!(2026 - 09 - 30)).unwrap(),
            AiUsagePeriod::Month,
            &stockholm(),
            Some(&company("company-1")),
        )
        .await
        .unwrap();
    let mut team = period_totals
        .iter()
        .map(|row| (row.project_key.as_str(), row.totals.priced_cost_usd))
        .collect::<Vec<_>>();
    team.sort_by(|a, b| a.0.cmp(b.0));
    let mut personal = report.projects[0]
        .keys
        .iter()
        .map(|key| (key.project.project_key.as_str(), key.totals.priced_cost_usd))
        .collect::<Vec<_>>();
    personal.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(personal, team);
    assert_eq!(personal, [(APP, 1.0), (other, 2.0)]);

    let sessions = service(&pool, true)
        .sessions(
            &user,
            &september(AiUsageFilter::default()),
            AiSessionOrder::Cost,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(sessions.sessions[0].totals.priced_cost_usd, 3.0);
}

#[sqlx::test]
async fn every_read_covers_only_the_users_own_usage(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let colleague = insert_user(&pool, "colleague@example.com", "Admin").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    record(&pool, dev, MACHINE_A, vec![bucket(hour, APP, 1, Some(0.1))]).await;
    record(
        &pool,
        colleague,
        MACHINE_B,
        vec![
            bucket(hour, APP, 100, Some(10.0)),
            bucket(hour, "github.com/example/secret", 100, None),
        ],
    )
    .await;
    let service = service(&pool, true);

    let report = service
        .report(&dev, &september(AiUsageFilter::default()))
        .await
        .unwrap();
    assert_eq!(report.totals.records, 1);
    assert_eq!(
        report
            .options
            .machines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [MACHINE_A]
    );
    let sessions = service
        .sessions(
            &dev,
            &september(AiUsageFilter::default()),
            AiSessionOrder::Cost,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        sessions
            .sessions
            .iter()
            .map(|session| (session.machine_id.to_string(), session.totals.records))
            .collect::<Vec<_>>(),
        [(MACHINE_A.to_string(), 1)]
    );
    let machines = service
        .machines(&dev, &september(AiUsageFilter::default()))
        .await
        .unwrap();
    assert_eq!(
        machines
            .machines
            .iter()
            .map(|machine| (
                machine.status.machine.id.to_string(),
                machine.usage.map(|usage| usage.records)
            ))
            .collect::<Vec<_>>(),
        [(MACHINE_A.to_string(), Some(1))]
    );
    let keys = service.project_keys(&dev).await.unwrap();
    assert_eq!(
        keys.keys
            .iter()
            .map(|key| key.project.project_key.as_str())
            .collect::<Vec<_>>(),
        [APP]
    );

    // Filtering on another user's machine finds nothing of theirs.
    let theirs = AiUsageFilter {
        machine: AiMachineId::parse(MACHINE_B),
        ..AiUsageFilter::default()
    };
    let filtered = service
        .report(&dev, &september(theirs.clone()))
        .await
        .unwrap();
    assert_eq!(filtered.totals, AiUsageTotals::default());
    let sessions = service
        .sessions(&dev, &september(theirs), AiSessionOrder::Cost, None, None)
        .await
        .unwrap();
    assert_eq!(sessions.total, 0);
}

#[sqlx::test]
async fn filters_narrow_every_sum_but_not_the_options(pool: PgPool) {
    let user = insert_user(&pool, "dev@example.com", "User").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    let new_key = "github.com/example/new";
    record(
        &pool,
        user,
        MACHINE_A,
        vec![
            variant(
                bucket(hour, APP, 1, Some(0.5)),
                "a",
                AiProvider::Claude,
                "model-x",
            ),
            variant(
                bucket(hour, UNATTRIBUTED_PROJECT_KEY, 4, None),
                "b",
                AiProvider::Claude,
                "",
            ),
        ],
    )
    .await;
    record(
        &pool,
        user,
        MACHINE_B,
        vec![variant(
            bucket(hour, new_key, 2, Some(0.25)),
            "c",
            AiProvider::Codex,
            "model-y",
        )],
    )
    .await;
    map(&pool, user, APP, "company-1", "101").await;
    let service = service(&pool, true);
    let records = |filter: AiUsageFilter| {
        let service = &service;
        async move {
            let report = service
                .report(&user, &september(filter.clone()))
                .await
                .unwrap();
            let sessions = service
                .sessions(
                    &user,
                    &september(filter),
                    AiSessionOrder::Recent,
                    None,
                    None,
                )
                .await
                .unwrap();
            // Every breakdown and the sessions follow the filter.
            for breakdown in [
                report
                    .days
                    .iter()
                    .map(|day| day.totals.records)
                    .sum::<i64>(),
                report.models.iter().map(|model| model.totals.records).sum(),
                report
                    .projects
                    .iter()
                    .map(|group| group.totals.records)
                    .sum(),
                report.hours.iter().map(|hour| hour.totals.records).sum(),
                sessions.sessions.iter().map(|s| s.totals.records).sum(),
            ] {
                assert_eq!(breakdown, report.totals.records);
            }
            assert_eq!(
                report.options.providers,
                [AiProvider::Codex, AiProvider::Claude]
            );
            assert_eq!(report.options.machines.len(), 2);
            report.totals.records
        }
    };

    assert_eq!(records(AiUsageFilter::default()).await, 7);
    assert_eq!(
        records(AiUsageFilter {
            provider: Some(AiProvider::Claude),
            ..AiUsageFilter::default()
        })
        .await,
        5
    );
    assert_eq!(
        records(AiUsageFilter {
            model: Some(String::new()),
            ..AiUsageFilter::default()
        })
        .await,
        4
    );
    assert_eq!(
        records(AiUsageFilter {
            project: Some(AiProjectFilter::Project(ProjectId::new("101"))),
            ..AiUsageFilter::default()
        })
        .await,
        1
    );
    assert_eq!(
        records(AiUsageFilter {
            project: Some(AiProjectFilter::Unassigned),
            ..AiUsageFilter::default()
        })
        .await,
        6
    );
    assert_eq!(
        records(AiUsageFilter {
            machine: AiMachineId::parse(MACHINE_B),
            ..AiUsageFilter::default()
        })
        .await,
        2
    );
    assert_eq!(
        records(AiUsageFilter {
            provider: Some(AiProvider::Codex),
            machine: AiMachineId::parse(MACHINE_A),
            ..AiUsageFilter::default()
        })
        .await,
        0
    );
}

#[sqlx::test]
async fn sessions_are_ranked_and_paged_without_gaps_or_repeats(pool: PgPool) {
    let user = insert_user(&pool, "dev@example.com", "User").await;
    let at = |hour: u8| datetime!(2026-09-22 00:00 UTC).replace_hour(hour).unwrap();
    // Five sessions; "b" and "c" tie on cost and on hours.
    let session = |key: &str, hour, records, cost| {
        variant(
            bucket(at(hour), APP, records, cost),
            key,
            AiProvider::Claude,
            "m",
        )
    };
    record(
        &pool,
        user,
        MACHINE_A,
        vec![
            session("a", 8, 1, Some(0.3)),
            session("b", 9, 2, Some(0.2)),
            session("c", 9, 3, Some(0.2)),
            session("d", 10, 4, None),
            session("e", 11, 5, Some(0.1)),
            // A second model of "a", an hour later.
            variant(
                bucket(at(12), APP, 1, Some(0.3)),
                "a",
                AiProvider::Claude,
                "n",
            ),
        ],
    )
    .await;
    let service = service(&pool, true);
    let listing = |order, page_size| {
        let service = &service;
        async move {
            let mut keys = Vec::new();
            let mut after: Option<String> = None;
            loop {
                let page = service
                    .sessions(
                        &user,
                        &september(AiUsageFilter::default()),
                        order,
                        after.as_deref(),
                        Some(page_size),
                    )
                    .await
                    .unwrap();
                assert_eq!(page.total, 5);
                keys.extend(
                    page.sessions
                        .iter()
                        .map(|session| session.session_key.trim_start_matches('0').to_string()),
                );
                match page.next {
                    Some(next) => after = Some(next.encode()),
                    None => break keys,
                }
            }
        }
    };

    for page_size in [1, 2, 5, 100] {
        assert_eq!(
            listing(AiSessionOrder::Cost, page_size).await,
            ["a", "c", "b", "e", "d"],
            "by cost in pages of {page_size}"
        );
        assert_eq!(
            listing(AiSessionOrder::Recent, page_size).await,
            ["a", "e", "d", "c", "b"],
            "by recency in pages of {page_size}"
        );
        assert_eq!(
            listing(AiSessionOrder::Tokens, page_size).await,
            ["e", "d", "c", "a", "b"],
            "by tokens in pages of {page_size}"
        );
    }

    let first = service
        .sessions(
            &user,
            &september(AiUsageFilter::default()),
            AiSessionOrder::Cost,
            None,
            None,
        )
        .await
        .unwrap();
    let a = &first.sessions[0];
    assert_eq!(a.models, ["m", "n"]);
    assert_eq!((a.first_hour, a.last_hour), (at(8), at(12)));
    assert_eq!(a.totals.priced_cost_usd, 0.6);
    assert_eq!(first.sessions[4].totals.estimated_cost_usd(), None);
}

#[sqlx::test]
async fn machines_report_coverage_staleness_and_all_their_usage_in_the_range(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let hour = datetime!(2026-09-22 08:00 UTC);
    record(
        &pool,
        dev,
        MACHINE_A,
        vec![
            bucket(hour, APP, 1, Some(0.1)),
            variant(bucket(hour, APP, 2, None), "f", AiProvider::Codex, "m"),
        ],
    )
    .await;
    record(&pool, dev, MACHINE_B, Vec::new()).await;
    let now = OffsetDateTime::now_utc();
    set_last_synced_at(&pool, MACHINE_A, now - Duration::days(8)).await;
    set_last_synced_at(&pool, MACHINE_B, now - Duration::days(6)).await;
    let service = service(&pool, true);

    // A report filter never narrows a machine's usage.
    let machines = service
        .machines(
            &dev,
            &september(AiUsageFilter {
                provider: Some(AiProvider::Claude),
                ..AiUsageFilter::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        machines
            .machines
            .iter()
            .map(|machine| (
                machine.status.machine.id.to_string(),
                machine.stale,
                machine
                    .usage
                    .map(|usage| (usage.records, usage.unpriced_records))
            ))
            .collect::<Vec<_>>(),
        [
            (MACHINE_B.to_string(), false, None),
            (MACHINE_A.to_string(), true, Some((3, 2))),
        ]
    );
    assert_eq!(
        machines.machines[1]
            .status
            .coverage
            .iter()
            .map(|entry| (
                entry.coverage.provider,
                entry.coverage.status,
                entry.coverage.unreadable,
                entry.has_usage,
                entry.pricing.as_ref().map(|pricing| pricing.status),
            ))
            .collect::<Vec<_>>(),
        [
            (
                AiProvider::Codex,
                AiCoverageStatus::Ok,
                0,
                true,
                Some(AiPricingStatus::Fresh)
            ),
            (
                AiProvider::Claude,
                AiCoverageStatus::Ok,
                0,
                true,
                Some(AiPricingStatus::Fresh)
            ),
            (AiProvider::Grok, AiCoverageStatus::Missing, 1, false, None),
        ]
    );

    let october = service
        .machines(
            &dev,
            &query(
                date!(2026 - 10 - 01),
                date!(2026 - 10 - 31),
                AiUsageFilter::default(),
            ),
        )
        .await
        .unwrap();
    assert!(october
        .machines
        .iter()
        .all(|machine| machine.usage.is_none()));
}

#[sqlx::test]
async fn keys_are_attributed_by_their_current_mapping(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    let day = |day: u8| datetime!(2026-09-01 08:00 UTC).replace_day(day).unwrap();
    record(
        &pool,
        dev,
        MACHINE_A,
        vec![
            bucket(day(20), APP, 1, Some(0.1)),
            bucket(day(21), "github.com/example/old", 1, Some(0.1)),
            bucket(day(22), "github.com/example/new", 1, Some(0.1)),
            bucket(day(23), UNATTRIBUTED_PROJECT_KEY, 1, None),
            bucket(day(19), APP, 1, Some(0.1)),
        ],
    )
    .await;
    map(&pool, dev, APP, "company-1", "101").await;
    map(&pool, dev, "github.com/example/old", "company-2", "909").await;

    let keys = service(&pool, true).project_keys(&dev).await.unwrap();
    assert!(keys.time_tracking_configured);
    let project = AiMappedProject {
        id: ProjectId::new("101"),
        name: "Project 101".to_string(),
    };
    assert_eq!(
        keys.keys
            .iter()
            .map(|key| (
                key.project.project_key.as_str(),
                key.last_used_on,
                key.project.resolution.clone()
            ))
            .collect::<Vec<_>>(),
        [
            (
                UNATTRIBUTED_PROJECT_KEY,
                date!(2026 - 09 - 23),
                AiProjectResolution::Unattributed
            ),
            (
                "github.com/example/new",
                date!(2026 - 09 - 22),
                AiProjectResolution::Unmapped { mappable: true }
            ),
            (
                "github.com/example/old",
                date!(2026 - 09 - 21),
                AiProjectResolution::Stale
            ),
            (
                APP,
                date!(2026 - 09 - 20),
                AiProjectResolution::Mapped(project)
            ),
        ]
    );

    // Without time tracking, mapped keys are unconfigured rather than
    // stale, and nothing can be mapped.
    let unconfigured = service(&pool, false).project_keys(&dev).await.unwrap();
    assert!(!unconfigured.time_tracking_configured);
    assert_eq!(
        unconfigured
            .keys
            .iter()
            .map(|key| key.project.resolution.clone())
            .collect::<Vec<_>>(),
        [
            AiProjectResolution::Unattributed,
            AiProjectResolution::Unmapped { mappable: false },
            AiProjectResolution::Unconfigured,
            AiProjectResolution::Unconfigured,
        ]
    );
    let report = service(&pool, false)
        .report(&dev, &september(AiUsageFilter::default()))
        .await
        .unwrap();
    assert!(!report.time_tracking_configured);
    assert_eq!(report.projects.len(), 1);
    assert_eq!(report.projects[0].project, None);
}
