use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    Duration,
};

use super::*;
use crate::{
    adapters::outbound::postgres::{
        ai_usage_fixtures::insert_user, PostgresAiProviderAccountRepository,
    },
    domain::{
        models::{AiProviderLogin, AiSourceStatus},
        ports::outbound::AiProviderAccountRepository,
    },
};

const COPILOT: AiProvider = AiProvider::Copilot;
const FETCHED: OffsetDateTime = datetime!(2026-10-06 03:00 UTC);

fn repository(pool: &PgPool) -> PostgresAiMeteredUsageRepository {
    PostgresAiMeteredUsageRepository::new(DbPool::from(pool.clone()))
}

fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap()
}

fn account(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
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

fn credit(label: &str, model: &str, kind: AiCreditKind, quantity: &str) -> AiMeteredCredit {
    AiMeteredCredit {
        product: "Copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "ai-credits".to_string(),
        model: AiMeteredModel {
            label: label.to_string(),
            model: model.to_string(),
            auto_routed: label.starts_with("Auto: "),
        },
        kind,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec(quantity),
            gross_amount: dec(quantity),
            discount_quantity: dec(quantity),
            discount_amount: dec("0.00001"),
            net_quantity: AiDecimal::ZERO,
            net_amount: AiDecimal::ZERO,
        },
    }
}

fn sol(quantity: &str) -> AiMeteredCredit {
    credit(
        "GPT-6.1 Sol",
        "gpt-6.1-sol",
        AiCreditKind::Trackable,
        quantity,
    )
}

/// Every stored account usage row as text, in key order.
async fn usage_rows(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT row_to_json(usage)::text FROM ai_provider_account_usage AS usage
         ORDER BY account_id, day, product, sku, model_label",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test]
async fn replacing_a_day_writes_only_changed_content(pool: PgPool) {
    insert_account(&pool, 1001, "ada-lovelace").await;
    let repository = repository(&pool);
    let login = AiProviderLogin::parse("ada-lovelace").unwrap();
    let scope = AiCreditStoreScope::Account {
        id: account(1001),
        login: &login,
    };
    let day = date!(2026 - 10 - 05);
    let other_day = date!(2026 - 10 - 06);

    let changed = repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[
                (
                    day,
                    vec![
                        sol("12.345678912"),
                        credit("GPT-5.4", "gpt-5.4", AiCreditKind::Trackable, "3"),
                    ],
                ),
                (other_day, vec![sol("1")]),
            ],
            FETCHED,
        )
        .await
        .unwrap();
    assert_eq!(changed, 2);
    let stored = repository
        .account_credits(COPILOT, october().dates(), None)
        .await
        .unwrap();
    assert_eq!(stored.len(), 3);
    let first = &stored[0];
    assert_eq!(first.login, "ada-lovelace");
    assert_eq!(first.credit.model.label, "GPT-5.4");
    assert_eq!(
        stored[1].credit.amounts.gross_quantity.to_string(),
        "12.345678912"
    );
    assert_eq!(
        stored[1].credit.amounts.discount_amount.to_string(),
        "0.00001"
    );
    assert_eq!(stored[1].gross_usd.nanos(), 123_456_789);

    // The same content again changes nothing, fetched_at included.
    let before = usage_rows(&pool).await;
    let changed = repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[
                (
                    day,
                    vec![
                        credit("GPT-5.4", "gpt-5.4", AiCreditKind::Trackable, "3.0"),
                        sol("12.345678912"),
                    ],
                ),
                (other_day, vec![sol("1")]),
            ],
            FETCHED + Duration::hours(1),
        )
        .await
        .unwrap();
    assert_eq!(changed, 0);
    assert_eq!(usage_rows(&pool).await, before);

    // A row restated under another label replaces the old label's row.
    let changed = repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(
                day,
                vec![
                    credit(
                        "Auto: GPT-6.1 Sol",
                        "gpt-6.1-sol",
                        AiCreditKind::Trackable,
                        "12.345678912",
                    ),
                    credit("GPT-5.4", "gpt-5.4", AiCreditKind::Trackable, "3"),
                ],
            )],
            FETCHED + Duration::hours(2),
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);
    let labels: Vec<String> = repository
        .account_credits(
            COPILOT,
            AiUsageDateRange::from_inclusive(day, day).unwrap(),
            None,
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.credit.model.label)
        .collect();
    assert_eq!(labels, ["Auto: GPT-6.1 Sol", "GPT-5.4"]);

    // An empty day deletes that day's rows only.
    repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(day, Vec::new())],
            FETCHED + Duration::hours(3),
        )
        .await
        .unwrap();
    let left = repository
        .account_credits(COPILOT, october().dates(), Some(&[account(1001)]))
        .await
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].day, other_day);

    let totals = repository
        .credit_month_totals(COPILOT, AiStoredScope::Account(account(1001)), october())
        .await
        .unwrap();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].gross_quantity, dec("1"));
    // Item 3 reads the same days.
    let days = PostgresAiProviderAccountRepository::new(DbPool::from(pool.clone()))
        .account_credit_days(COPILOT, october().dates())
        .await
        .unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].gross.nanos(), 10_000_000);
}

#[sqlx::test]
async fn slices_record_completeness_and_late_changes(pool: PgPool) {
    let repository = repository(&pool);
    let day = date!(2026 - 10 - 05);
    let scope = AiCreditStoreScope::Organization;
    let slice = |records: Vec<AiSliceRecord>| records.into_iter().find(|s| s.day == day).unwrap();
    let read = || repository.slices(COPILOT, AiMeteredSource::OrgCredits, october().dates());

    // During the day: fetched, not complete.
    repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(day, vec![sol("1")])],
            datetime!(2026-10-05 20:00 UTC),
        )
        .await
        .unwrap();
    let during = slice(read().await.unwrap());
    assert_eq!(during.account_id, None);
    assert_eq!(during.complete_at, None);
    assert_eq!(during.changed_at, datetime!(2026-10-05 20:00 UTC));

    // After the day with other content: complete, but not late (the fetch
    // before was during the day).
    repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(day, vec![sol("2")])],
            datetime!(2026-10-06 01:00 UTC),
        )
        .await
        .unwrap();
    let after = slice(read().await.unwrap());
    assert_eq!(after.complete_at, Some(datetime!(2026-10-06 01:00 UTC)));
    assert_eq!(after.late_changed_at, None);
    assert_eq!(after.changed_at, datetime!(2026-10-06 01:00 UTC));

    // Unchanged later: still complete from the first fetch after the day.
    repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(day, vec![sol("2")])],
            datetime!(2026-10-07 01:00 UTC),
        )
        .await
        .unwrap();
    let unchanged = slice(read().await.unwrap());
    assert_eq!(unchanged.fetched_at, datetime!(2026-10-07 01:00 UTC));
    assert_eq!(unchanged.changed_at, datetime!(2026-10-06 01:00 UTC));
    assert_eq!(unchanged.complete_at, Some(datetime!(2026-10-06 01:00 UTC)));
    assert_eq!(unchanged.late_changed_at, None);

    // Changed after a complete fetch: a late change.
    repository
        .replace_credit_days(
            COPILOT,
            scope,
            &[(day, vec![sol("2.5")])],
            datetime!(2026-10-08 05:00 UTC),
        )
        .await
        .unwrap();
    let late = slice(read().await.unwrap());
    assert_eq!(late.late_changed_at, Some(datetime!(2026-10-08 05:00 UTC)));
    assert_eq!(late.complete_at, Some(datetime!(2026-10-06 01:00 UTC)));

    let totals = repository
        .credit_month_totals(COPILOT, AiStoredScope::Organization, october())
        .await
        .unwrap();
    assert_eq!(totals[0].gross_quantity, dec("2.5"));
    assert_eq!(repository.months(COPILOT).await.unwrap(), [october()]);
}

#[sqlx::test]
async fn toki_s_own_fields_are_rewritten_without_changing_the_slice(pool: PgPool) {
    insert_account(&pool, 1001, "ada-lovelace").await;
    let repository = repository(&pool);
    let day = date!(2026 - 10 - 05);
    let complete_fetch = datetime!(2026-10-06 01:00 UTC);
    let slice = || async {
        repository
            .slices(COPILOT, AiMeteredSource::AccountCredits, october().dates())
            .await
            .unwrap()
            .into_iter()
            .find(|slice| slice.day == day)
            .unwrap()
    };
    let unknown = |model: &str, kind| credit("Mystery Model", model, kind, "2");
    let ada = AiProviderLogin::parse("ada-lovelace").unwrap();
    let scope = |login| AiCreditStoreScope::Account {
        id: account(1001),
        login,
    };

    // Fetched once after the day ended: complete.
    repository
        .replace_credit_days(
            COPILOT,
            scope(&ada),
            &[(day, vec![sol("1"), unknown("", AiCreditKind::Unrecognized)])],
            complete_fetch,
        )
        .await
        .unwrap();

    // The account was renamed and a new label override maps the unknown
    // label, but GitHub sent the same rows: rewritten, not a late change.
    let renamed = AiProviderLogin::parse("ada-byron").unwrap();
    let changed = repository
        .replace_credit_days(
            COPILOT,
            scope(&renamed),
            &[(
                day,
                vec![sol("1"), unknown("mystery-model", AiCreditKind::Trackable)],
            )],
            datetime!(2026-10-08 05:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(changed, 0);
    let after = slice().await;
    assert_eq!(after.late_changed_at, None);
    assert_eq!(after.changed_at, complete_fetch);
    assert_eq!(after.fetched_at, datetime!(2026-10-08 05:00 UTC));
    let stored = repository
        .account_credits(COPILOT, october().dates(), None)
        .await
        .unwrap();
    assert!(stored.iter().all(|row| row.login == "ada-byron"));
    assert!(stored.iter().all(|row| row.fetched_at == complete_fetch));
    let mystery = stored
        .iter()
        .find(|row| row.credit.model.label == "Mystery Model")
        .unwrap();
    assert_eq!(mystery.credit.model.model, "mystery-model");
    assert_eq!(mystery.credit.kind, AiCreditKind::Trackable);

    // What GitHub sends changing afterwards is a late change.
    let changed = repository
        .replace_credit_days(
            COPILOT,
            scope(&renamed),
            &[(
                day,
                vec![
                    sol("1.5"),
                    unknown("mystery-model", AiCreditKind::Trackable),
                ],
            )],
            datetime!(2026-10-09 05:00 UTC),
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);
    assert_eq!(
        slice().await.late_changed_at,
        Some(datetime!(2026-10-09 05:00 UTC))
    );
}

fn charge(
    day: Date,
    repository_name: &str,
    kind: AiOrgChargeKind,
    quantity: &str,
) -> AiMeteredOrgCharge {
    AiMeteredOrgCharge {
        day,
        product: "copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "AICredits".to_string(),
        repository: repository_name.to_string(),
        kind,
        unit_price: dec("0.01"),
        quantity: dec(quantity),
        gross_amount: dec(quantity),
        discount_amount: AiDecimal::ZERO,
        net_amount: dec(quantity),
        reported_at: Some(day.midnight().assume_utc()),
    }
}

#[sqlx::test]
async fn org_usage_and_summaries_replace_their_days_and_month(pool: PgPool) {
    let repository = repository(&pool);
    let days =
        AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), date!(2026 - 10 - 05)).unwrap();
    let rows = vec![
        charge(
            date!(2026 - 10 - 02),
            "widget-api",
            AiOrgChargeKind::RepositoryCredits,
            "60",
        ),
        charge(
            date!(2026 - 10 - 02),
            "",
            AiOrgChargeKind::AccountCreditsAggregate,
            "900",
        ),
    ];
    assert_eq!(
        repository
            .replace_org_usage(COPILOT, days, &rows, FETCHED)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repository
            .replace_org_usage(COPILOT, days, &rows, FETCHED)
            .await
            .unwrap(),
        0
    );
    let stored = repository
        .org_charges(COPILOT, october().dates())
        .await
        .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].repository, "");
    assert_eq!(stored[1].credit_usd().unwrap().nanos(), 600_000_000);
    // Every day of the range has a slice.
    assert_eq!(
        repository
            .slices(COPILOT, AiMeteredSource::OrgUsage, october().dates())
            .await
            .unwrap()
            .len(),
        5
    );
    // A row outside the replaced days is refused.
    assert!(repository
        .replace_org_usage(
            COPILOT,
            days,
            &[charge(
                date!(2026 - 10 - 09),
                "widget-api",
                AiOrgChargeKind::RepositoryCredits,
                "1"
            )],
            FETCHED
        )
        .await
        .is_err());

    let line = AiMeteredSummaryLine {
        product: "copilot".to_string(),
        sku: "copilot_ai_unit".to_string(),
        unit: "ai-units".to_string(),
        kind: AiSummaryKind::Credits,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec("4000"),
            gross_amount: dec("40"),
            discount_quantity: dec("3800"),
            discount_amount: dec("38"),
            net_quantity: dec("200"),
            net_amount: dec("2"),
        },
    };
    assert!(repository
        .replace_summary(COPILOT, october(), std::slice::from_ref(&line), FETCHED)
        .await
        .unwrap());
    assert!(!repository
        .replace_summary(
            COPILOT,
            october(),
            std::slice::from_ref(&line),
            FETCHED + Duration::hours(1)
        )
        .await
        .unwrap());
    let (lines, fetched_at) = repository
        .summary(COPILOT, october())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lines, std::slice::from_ref(&line));
    assert_eq!(fetched_at, FETCHED + Duration::hours(1));

    // Toki classifying the same rows otherwise (a code change) is no change.
    let reclassified = AiMeteredSummaryLine {
        kind: AiSummaryKind::Unrecognized,
        ..line
    };
    assert!(!repository
        .replace_summary(
            COPILOT,
            october(),
            std::slice::from_ref(&reclassified),
            FETCHED + Duration::hours(2)
        )
        .await
        .unwrap());
    let (lines, _) = repository
        .summary(COPILOT, october())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lines, [reclassified]);
    let recharged: Vec<AiMeteredOrgCharge> = rows
        .iter()
        .map(|row| AiMeteredOrgCharge {
            kind: match row.kind {
                AiOrgChargeKind::RepositoryCredits => AiOrgChargeKind::Unrecognized,
                kind => kind,
            },
            ..row.clone()
        })
        .collect();
    assert_eq!(
        repository
            .replace_org_usage(COPILOT, days, &recharged, FETCHED + Duration::hours(2))
            .await
            .unwrap(),
        0
    );
    let stored = repository
        .org_charges(COPILOT, october().dates())
        .await
        .unwrap();
    assert_eq!(stored[1].kind, AiOrgChargeKind::Unrecognized);
    assert_eq!(stored[1].credit_usd(), None);
    let late: Vec<AiSliceRecord> = repository
        .slices(COPILOT, AiMeteredSource::OrgUsage, october().dates())
        .await
        .unwrap()
        .into_iter()
        .filter(|slice| slice.changed_at != FETCHED || slice.late_changed_at.is_some())
        .collect();
    assert_eq!(late, []);
    assert_eq!(
        repository
            .summary(COPILOT, AiBillingMonth::parse("2026-09").unwrap())
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test]
async fn constraints_refuse_what_the_import_never_writes(pool: PgPool) {
    let repository = repository(&pool);
    let login = AiProviderLogin::parse("nobody-known").unwrap();
    // An account Toki never listed.
    let error = repository
        .replace_credit_days(
            COPILOT,
            AiCreditStoreScope::Account {
                id: account(4040),
                login: &login,
            },
            &[(date!(2026 - 10 - 05), vec![sol("1")])],
            FETCHED,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AiMeteredUsageError::Storage(message)
            if message.contains("ai_provider_account_usage_account_fkey")),
        "{error}"
    );
    for (statement, constraint) in [
        (
            "INSERT INTO ai_provider_org_credits (provider, day, product, sku, model_label, model,
                auto_routed, kind, unit_type, unit_price, gross_quantity, gross_amount,
                discount_quantity, discount_amount, net_quantity, net_amount, gross_usd_nanos,
                fetched_at)
             VALUES ('copilot', '2026-10-05', 'Copilot', 'Copilot AI Credits', 'GPT-6.1 Sol',
                'gpt-6.1-sol', false, 'trackable', 'ai-credits', 0.01, -1, 0, 0, 0, 0, 0, 0, now())",
            "ai_provider_org_credits_amounts_check",
        ),
        (
            "INSERT INTO ai_provider_org_usage (provider, day, product, sku, unit_type, repository,
                kind, unit_price, quantity, gross_amount, discount_amount, net_amount,
                credit_usd_nanos, fetched_at)
             VALUES ('copilot', '2026-10-05', 'copilot', 'Copilot AI Credits', 'AICredits', '',
                'repository_credits', 0.01, 1, 0.01, 0, 0.01, 10000000, now())",
            "ai_provider_org_usage_repository_check",
        ),
        (
            "INSERT INTO ai_provider_usage_slices (provider, source, account_id, day, fetched_at,
                changed_at)
             VALUES ('copilot', 'org_credits', 5, '2026-10-05', now(), now())",
            "ai_provider_usage_slices_account_check",
        ),
    ] {
        let error = sqlx::query(statement).execute(&pool).await.unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|error| error.constraint()),
            Some(constraint),
            "{error}"
        );
    }
}

#[sqlx::test]
async fn one_import_holds_the_lock_at_a_time(pool: PgPool) {
    let repository = repository(&pool);
    let first = repository.try_lock_import(COPILOT).await.unwrap().unwrap();
    assert!(repository.try_lock_import(COPILOT).await.unwrap().is_none());
    first.release().await.unwrap();
    let third = repository.try_lock_import(COPILOT).await.unwrap();
    assert!(third.is_some());
    // Dropping a lease frees the lock as well.
    drop(third);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(repository.try_lock_import(COPILOT).await.unwrap().is_some());
}

fn plan() -> AiImportPlan {
    AiImportPlan {
        kind: AiImportKind::Hourly,
        months: vec![
            (AiBillingMonth::parse("2026-09").unwrap(), AiMonthMode::Full),
            (october(), AiMonthMode::Changes),
        ],
    }
}

#[sqlx::test]
async fn runs_are_recorded_and_leftovers_marked_interrupted(pool: PgPool) {
    let repository = repository(&pool);
    let lease = repository.try_lock_import(COPILOT).await.unwrap().unwrap();
    let first = repository
        .start_run(COPILOT, &plan(), FETCHED)
        .await
        .unwrap();
    let running = repository.import_status(COPILOT).await.unwrap();
    assert_eq!(running.running.as_ref().unwrap().id, first);
    assert!(running.last_run.is_none());

    // The process dies mid-run (a deploy): the lock is free, so the row that
    // still says running is not running.
    drop(lease);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let orphaned = repository.import_status(COPILOT).await.unwrap();
    assert!(orphaned.running.is_none());
    assert!(orphaned.last_run.is_none());

    // Whoever holds the lock next proves the first run dead.
    let lease = repository.try_lock_import(COPILOT).await.unwrap().unwrap();
    let started_at = FETCHED + Duration::hours(1);
    let second = repository
        .start_run(COPILOT, &plan(), started_at)
        .await
        .unwrap();
    let status = repository.import_status(COPILOT).await.unwrap();
    let interrupted = status.last_run.unwrap();
    assert_eq!(interrupted.id, first);
    assert_eq!(interrupted.status, AiRunStatus::Failed);
    assert_eq!(interrupted.error.as_deref(), Some("interrupted"));
    assert_eq!(status.running.unwrap().id, second);

    insert_account(&pool, 1001, "ada-lovelace").await;
    repository
        .record_check(
            COPILOT,
            AiMeteredSource::AccountCredits,
            october(),
            &AiUsageCheck {
                account_id: Some(account(1001)),
                checked_at: started_at + Duration::seconds(5),
                payer_gross: None,
                stored_gross: dec("1.5"),
                matches: None,
                error: Some("GitHub knows no Copilot usage for ada-lovelace".to_string()),
            },
        )
        .await
        .unwrap();
    repository
        .record_check(
            COPILOT,
            AiMeteredSource::OrgCredits,
            october(),
            &AiUsageCheck {
                account_id: None,
                checked_at: started_at + Duration::seconds(5),
                payer_gross: Some(dec("12.345678912")),
                stored_gross: dec("12.345678912"),
                matches: Some(true),
                error: None,
            },
        )
        .await
        .unwrap();
    let run = AiImportRun {
        id: second,
        kind: AiImportKind::Hourly,
        months: plan().months,
        started_at,
        finished_at: Some(started_at + Duration::minutes(1)),
        status: AiRunStatus::Partial,
        requests: 12,
        slices_fetched: 30,
        slices_changed: 2,
        account_errors: 1,
        error: None,
        sources: AiMeteredSource::ALL
            .into_iter()
            .map(|source| AiSourceOutcome {
                source,
                status: if source == AiMeteredSource::AccountCredits {
                    AiSourceStatus::Partial
                } else {
                    AiSourceStatus::Succeeded
                },
                requests: 3,
                slices_fetched: 7,
                slices_changed: 1,
                imported_through: Some(date!(2026 - 10 - 04)),
                settle_lag_days: (source == AiMeteredSource::AccountCredits).then_some(2),
                error: (source == AiMeteredSource::AccountCredits)
                    .then(|| "1 account imports failed".to_string()),
            })
            .collect(),
    };
    repository.finish_run(COPILOT, &run).await.unwrap();
    lease.release().await.unwrap();

    let status = repository.import_status(COPILOT).await.unwrap();
    assert!(status.running.is_none());
    let last = status.last_run.clone().unwrap();
    assert_eq!(last.id, second);
    assert_eq!(last.months, plan().months);
    assert_eq!(last.sources.len(), 4);
    assert_eq!(status.history.last_success_at, run.finished_at);
    assert_eq!(status.history.last_nightly_success_at, None);
    assert_eq!(status.settle_days(), 3);
    assert_eq!(
        status.account_errors,
        [AiStoredAccountError {
            account_id: account(1001),
            month: october(),
            error: "GitHub knows no Copilot usage for ada-lovelace".to_string(),
        }]
    );
    let accounts = status
        .sources
        .iter()
        .find(|state| state.source == AiMeteredSource::AccountCredits)
        .unwrap();
    assert_eq!(accounts.imported_through, Some(date!(2026 - 10 - 04)));
    assert_eq!(accounts.settle_lag_days, Some(2));
    assert_eq!(accounts.last_success_at, None);
    let org = status
        .sources
        .iter()
        .find(|state| state.source == AiMeteredSource::OrgCredits)
        .unwrap();
    assert_eq!(org.last_success_at, run.finished_at);

    let checks = repository.checks(COPILOT, october()).await.unwrap();
    assert_eq!(checks.len(), 2);
    let org_check = checks
        .iter()
        .find(|check| check.account_id.is_none())
        .unwrap();
    assert_eq!(org_check.payer_gross, Some(dec("12.345678912")));
    assert_eq!(org_check.matches, Some(true));
}

const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
const OTHER_MACHINE: &str = "6a1d6b2f-4c9f-4e9f-8b68-1e8c2d2f3f4b";

async fn insert_machine(pool: &PgPool, user: UserId, machine: &str) {
    sqlx::query(
        "INSERT INTO ai_usage_machines (id, user_id, label, client_version, time_zone)
         VALUES ($1::uuid, $2, 'laptop', '0.10.0', 'Europe/Stockholm')",
    )
    .bind(machine)
    .bind(user.as_i32())
    .execute(pool)
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn insert_bucket(
    pool: &PgPool,
    user: UserId,
    machine: &str,
    hour: OffsetDateTime,
    provider: &str,
    payer: Option<&str>,
    model: &str,
    cost: Option<f64>,
    basis: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO ai_usage_buckets (user_id, machine_id, hour_start, session_key, project_key,
            provider, model, input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
            records, estimated_cost_usd, unpriced_records, billing_provider, pricing_basis)
         VALUES ($1, $2::uuid, $3, '9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f', 'github.com/example/widget-api',
            $4, $5, 10, 100, 5, 7, 2, $6::float8::numeric, $7, $8, $9)",
    )
    .bind(user.as_i32())
    .bind(machine)
    .bind(hour)
    .bind(provider)
    .bind(model)
    .bind(cost)
    .bind(if cost.is_none() { 2_i64 } else { 0 })
    .bind(payer)
    .bind(basis)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test]
async fn tracked_usage_is_what_the_payer_paid_for_with_the_old_basis_apart(pool: PgPool) {
    let ada = insert_user(&pool, "ada@example.com", "User").await;
    let grace = insert_user(&pool, "grace@example.com", "User").await;
    insert_machine(&pool, ada, MACHINE).await;
    insert_machine(&pool, grace, OTHER_MACHINE).await;
    // Ada's personal key still counts: the payer paid for it.
    sqlx::query(
        "INSERT INTO ai_personal_project_keys (user_id, project_key)
         VALUES ($1, 'github.com/example/widget-api')",
    )
    .bind(ada.as_i32())
    .execute(&pool)
    .await
    .unwrap();
    let floor = datetime!(2026-10-01 00:00 UTC);
    let new_basis = Some("copilot-rates@2026-10-24");
    // Ada's machine: Copilot on an old basis, then on the new one; its latest
    // basis comes from OpenCode routed through Copilot.
    for (hour, provider, payer, model, cost, basis) in [
        // Before the floor: never repriced.
        (
            datetime!(2026-09-30 23:00 UTC),
            "copilot",
            None,
            "gpt-6.1-sol",
            Some(1.0),
            None,
        ),
        // 23:00 and 00:00 UTC fall on different days.
        (
            datetime!(2026-10-04 23:00 UTC),
            "copilot",
            None,
            "gpt-6.1-sol",
            Some(2.0),
            None,
        ),
        (
            datetime!(2026-10-05 00:00 UTC),
            "copilot",
            None,
            "gpt-6.1-sol",
            Some(3.0),
            Some("copilot-rates@2026-10-01"),
        ),
        (
            datetime!(2026-10-05 01:00 UTC),
            "copilot",
            None,
            "gpt-6.1-sol",
            Some(4.0),
            new_basis,
        ),
        (
            datetime!(2026-10-05 02:00 UTC),
            "opencode",
            Some("copilot"),
            "gpt-6.1-sol",
            Some(0.5),
            new_basis,
        ),
        // Unpriced usage has no cost.
        (
            datetime!(2026-10-05 03:00 UTC),
            "copilot",
            None,
            "claude-sonnet-5.5",
            None,
            None,
        ),
        // Copilot routed to Codex is not Copilot's.
        (
            datetime!(2026-10-05 04:00 UTC),
            "copilot",
            Some("codex"),
            "gpt-6.1-sol",
            Some(9.0),
            None,
        ),
    ] {
        insert_bucket(
            &pool, ada, MACHINE, hour, provider, payer, model, cost, basis,
        )
        .await;
    }
    // Grace's machine has never sent a basis.
    insert_bucket(
        &pool,
        grace,
        OTHER_MACHINE,
        datetime!(2026-10-05 10:00 UTC),
        "copilot",
        None,
        "gpt-5.3-codex",
        Some(1.5),
        None,
    )
    .await;

    let dates =
        AiUsageDateRange::from_inclusive(date!(2026 - 09 - 30), date!(2026 - 10 - 05)).unwrap();
    let repository = repository(&pool);
    let rows = repository
        .tracked_usage(COPILOT, dates, floor, None)
        .await
        .unwrap();
    let find = |user: UserId, day: Date, model: &str| {
        rows.iter()
            .find(|row| row.user_id == user && row.day == day && row.model == model)
            .unwrap_or_else(|| panic!("no {user:?} {day} {model}"))
    };
    let usd = |dollars: i64| dollars * 1_000_000_000;

    let before_floor = find(ada, date!(2026 - 09 - 30), "gpt-6.1-sol");
    assert_eq!(before_floor.priced_cost.nanos(), usd(1));
    assert_eq!(before_floor.before_floor_cost.nanos(), usd(1));
    assert_eq!(before_floor.old_basis_buckets, 0);

    let late = find(ada, date!(2026 - 10 - 04), "gpt-6.1-sol");
    assert_eq!(late.priced_cost.nanos(), usd(2));
    // No basis at or after the floor is old.
    assert_eq!(late.old_basis_cost.nanos(), usd(2));
    assert_eq!(late.old_basis_buckets, 1);

    let fifth = find(ada, date!(2026 - 10 - 05), "gpt-6.1-sol");
    // 3 + 4 + 0.5 (OpenCode through Copilot); Copilot through Codex left out.
    assert_eq!(fifth.priced_cost.nanos(), 7_500_000_000);
    // Only the bucket on a basis other than the machine's latest is old.
    assert_eq!(fifth.old_basis_cost.nanos(), usd(3));
    assert_eq!(fifth.old_basis_buckets, 1);
    assert_eq!(fifth.records, 6);

    let unpriced = find(ada, date!(2026 - 10 - 05), "claude-sonnet-5.5");
    assert_eq!(unpriced.priced_cost.nanos(), 0);
    assert_eq!(unpriced.unpriced_records, 2);
    assert_eq!(unpriced.old_basis_buckets, 1);

    let graces = find(grace, date!(2026 - 10 - 05), "gpt-5.3-codex");
    assert_eq!(graces.old_basis_cost.nanos(), 1_500_000_000);

    // One user's view.
    let only_grace = repository
        .tracked_usage(COPILOT, dates, floor, Some(grace))
        .await
        .unwrap();
    assert_eq!(only_grace.len(), 1);
}
