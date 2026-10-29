use std::sync::Arc;

use sqlx::PgPool;
use time::macros::{date, datetime};

use super::*;
use crate::{
    adapters::outbound::postgres::ai_usage_fixtures::{
        bucket, insert_user, record, APP, MACHINE_A, MACHINE_B,
    },
    domain::{
        models::{AiAdoptionCompanyDay, AiAdoptionSeat},
        ports::inbound::AiAdoptionService,
        services::AiAdoptionServiceImpl,
    },
};

fn repository(pool: &PgPool) -> PostgresAiAdoptionRepository {
    PostgresAiAdoptionRepository::new(DbPool::from(pool.clone()))
}

fn stockholm() -> AiUsageTimeZone {
    AiUsageTimeZone::parse("Europe/Stockholm").unwrap()
}

/// Copilot's credits measure its usage from September 2026 on.
fn credits() -> AiAdoptionCredits {
    AiAdoptionCredits {
        payer: AiProvider::Copilot,
        from: date!(2026 - 09 - 01),
    }
}

fn dates(from: Date, to: Date) -> AiUsageDateRange {
    AiUsageDateRange::from_inclusive(from, to).unwrap()
}

async fn insert_account(pool: &PgPool, id: i64) {
    sqlx::query(
        "INSERT INTO ai_provider_accounts (provider, account_id, login)
         VALUES ('copilot', $1, 'login-' || $1)",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

/// The account is `user`'s, or external without one, from `from` through `to`.
async fn dispose(pool: &PgPool, account: i64, user: Option<UserId>, from: Date, to: Option<Date>) {
    sqlx::query(
        "INSERT INTO ai_provider_account_dispositions
            (provider, account_id, login, target, user_id, valid_from, valid_to)
         VALUES ('copilot', $1, 'login-' || $1, $2, $3, $4, $5)",
    )
    .bind(account)
    .bind(if user.is_some() { "user" } else { "external" })
    .bind(user.map(|user| user.as_i32()))
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .unwrap();
}

/// Credits charged to an account on a UTC day, in billionths of a dollar.
async fn charge(pool: &PgPool, account: i64, day: Date, kind: AiCreditKind, nanos: i64) {
    sqlx::query(
        "INSERT INTO ai_provider_account_usage (provider, account_id, day, product, sku,
            model_label, model, auto_routed, kind, unit_type, unit_price, gross_quantity,
            gross_amount, discount_quantity, discount_amount, net_quantity, net_amount,
            gross_usd_nanos, login, fetched_at)
         VALUES ('copilot', $1, $2, 'Copilot', 'Copilot AI Credits', $3, 'gpt-6.1-sol', false,
            $3, 'ai-credits', 0.01, 1, 0.01, 0, 0, 1, 0.01, $4, 'login-' || $1, now())",
    )
    .bind(account)
    .bind(day)
    .bind(kind.as_str())
    .bind(nanos)
    .execute(pool)
    .await
    .unwrap();
}

async fn charge_company(pool: &PgPool, day: Date, nanos: i64) {
    sqlx::query(
        "INSERT INTO ai_provider_org_credits (provider, day, product, sku, model_label, model,
            auto_routed, kind, unit_type, unit_price, gross_quantity, gross_amount,
            discount_quantity, discount_amount, net_quantity, net_amount, gross_usd_nanos,
            fetched_at)
         VALUES ('copilot', $1, 'Copilot', 'Copilot AI Credits', 'GPT-6.1 Sol', 'gpt-6.1-sol',
            false, 'trackable', 'ai-credits', 0.01, 1, 0.01, 0, 0, 1, 0.01, $2, now())",
    )
    .bind(day)
    .bind(nanos)
    .execute(pool)
    .await
    .unwrap();
}

/// A day of the organization's credits (`account` 0) or of an account's.
async fn slice(pool: &PgPool, account: i64, day: Date, complete: bool) {
    sqlx::query(
        "INSERT INTO ai_provider_usage_slices (provider, source, account_id, day, fetched_at,
            changed_at, complete_at)
         VALUES ('copilot', CASE WHEN $1 = 0 THEN 'org_credits' ELSE 'account_credits' END, $1,
            $2, now(), now(), CASE WHEN $3 THEN now() END)",
    )
    .bind(account)
    .bind(day)
    .bind(complete)
    .execute(pool)
    .await
    .unwrap();
}

async fn through(pool: &PgPool, today: Date) -> Option<Date> {
    repository(pool)
        .credits_complete_through(&credits(), today)
        .await
        .unwrap()
}

/// A Copilot bucket of `model`, paid by Copilot unless `billing_provider` says.
fn copilot(hour_start: time::OffsetDateTime, model: &str, cost: f64) -> AiUsageBucket {
    AiUsageBucket {
        provider: AiProvider::Copilot,
        model: model.to_string(),
        ..bucket(hour_start, APP, 2, Some(cost))
    }
}

use crate::domain::models::AiUsageBucket;

#[sqlx::test]
async fn buckets_are_credited_from_the_first_instant_within_one_local_day(pool: PgPool) {
    let user = insert_user(&pool, "a@example.com", "User").await;
    // Stockholm's 1 September starts at 22:00 UTC the day before, two hours
    // before Copilot's credits start measuring.
    record(
        &pool,
        user,
        MACHINE_A,
        vec![
            copilot(datetime!(2026-08-31 22:00 UTC), "gpt-6.1-sol", 0.5),
            copilot(datetime!(2026-09-01 00:00 UTC), "gpt-6.1-sol", 0.25),
            bucket(datetime!(2026-09-01 08:00 UTC), APP, 3, Some(1.0)),
        ],
    )
    .await;

    let rows = repository(&pool)
        .usage(
            dates(date!(2026 - 09 - 01), date!(2026 - 09 - 01)),
            &stockholm(),
            Some(&credits()),
            None,
        )
        .await
        .unwrap();

    let summary: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row.day,
                row.provider,
                row.credited,
                row.usage.records,
                row.usage.priced_cost.nanos(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                date!(2026 - 09 - 01),
                AiProvider::Claude,
                false,
                3,
                1_000_000_000
            ),
            (
                date!(2026 - 09 - 01),
                AiProvider::Copilot,
                false,
                2,
                500_000_000
            ),
            (
                date!(2026 - 09 - 01),
                AiProvider::Copilot,
                true,
                2,
                250_000_000
            ),
        ]
    );

    // Without credits, nothing is credited.
    let rows = repository(&pool)
        .usage(
            dates(date!(2026 - 09 - 01), date!(2026 - 09 - 01)),
            &stockholm(),
            None,
            Some(&user),
        )
        .await
        .unwrap();
    assert!(rows.iter().all(|row| !row.credited));
}

#[sqlx::test]
async fn credits_belong_to_whoever_owned_the_account_that_day(pool: PgPool) {
    let first = insert_user(&pool, "a@example.com", "User").await;
    let second = insert_user(&pool, "b@example.com", "User").await;
    for account in [11, 12, 13] {
        insert_account(&pool, account).await;
    }
    dispose(
        &pool,
        11,
        Some(first),
        date!(2026 - 08 - 01),
        Some(date!(2026 - 09 - 30)),
    )
    .await;
    dispose(&pool, 11, Some(second), date!(2026 - 10 - 01), None).await;
    dispose(&pool, 12, None, date!(2026 - 08 - 01), None).await;
    // Before the credits measure anything.
    charge(
        &pool,
        11,
        date!(2026 - 08 - 31),
        AiCreditKind::Trackable,
        999,
    )
    .await;
    charge(
        &pool,
        11,
        date!(2026 - 09 - 15),
        AiCreditKind::Trackable,
        100,
    )
    .await;
    charge(
        &pool,
        11,
        date!(2026 - 10 - 05),
        AiCreditKind::Trackable,
        50,
    )
    .await;
    charge(
        &pool,
        11,
        date!(2026 - 10 - 05),
        AiCreditKind::Untrackable,
        5,
    )
    .await;
    // Code review is billed per repository: no one's.
    charge(
        &pool,
        11,
        date!(2026 - 10 - 05),
        AiCreditKind::RepositoryBilled,
        30,
    )
    .await;
    // External and unlinked accounts are no one's either.
    charge(&pool, 12, date!(2026 - 10 - 05), AiCreditKind::Trackable, 7).await;
    charge(&pool, 13, date!(2026 - 10 - 05), AiCreditKind::Trackable, 8).await;
    charge_company(&pool, date!(2026 - 08 - 31), 999).await;
    charge_company(&pool, date!(2026 - 09 - 15), 100).await;
    charge_company(&pool, date!(2026 - 10 - 05), 100).await;
    // The organization's and account 11's credits are complete from the
    // first day through 4 October; the organization's 5 October is not yet.
    let mut day = date!(2026 - 09 - 01);
    while day <= date!(2026 - 10 - 04) {
        slice(&pool, 0, day, true).await;
        slice(&pool, 11, day, true).await;
        day = day.next_day().unwrap();
    }
    slice(&pool, 0, date!(2026 - 10 - 05), false).await;
    let range = dates(date!(2026 - 08 - 01), date!(2026 - 10 - 31));

    let credited = repository(&pool)
        .user_credits(&credits(), range)
        .await
        .unwrap();
    assert_eq!(
        credited,
        vec![
            AiAdoptionCredit {
                user_id: first,
                day: date!(2026 - 09 - 15),
                amount: AiUsdNanos::new(100).unwrap(),
            },
            AiAdoptionCredit {
                user_id: second,
                day: date!(2026 - 10 - 05),
                amount: AiUsdNanos::new(55).unwrap(),
            },
        ]
    );
    assert_eq!(
        repository(&pool)
            .company_credits(&credits(), range)
            .await
            .unwrap(),
        vec![
            (date!(2026 - 09 - 15), AiUsdNanos::new(100).unwrap()),
            (date!(2026 - 10 - 05), AiUsdNanos::new(100).unwrap()),
        ]
    );
    assert_eq!(
        through(&pool, date!(2026 - 10 - 28)).await,
        Some(date!(2026 - 10 - 04))
    );
    assert_eq!(
        through(&pool, date!(2026 - 09 - 15)).await,
        Some(date!(2026 - 09 - 15))
    );
    // A day an account's import missed bounds it, whatever came later.
    sqlx::query(
        "DELETE FROM ai_provider_usage_slices WHERE account_id = 11 AND day = '2026-09-20'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        through(&pool, date!(2026 - 10 - 28)).await,
        Some(date!(2026 - 09 - 19))
    );
    // An account of someone's that never imported holds everyone back; one
    // that is not anyone's yet does not.
    insert_account(&pool, 14).await;
    dispose(&pool, 14, Some(first), date!(2026 - 10 - 01), None).await;
    assert_eq!(through(&pool, date!(2026 - 10 - 28)).await, None);
    assert_eq!(
        through(&pool, date!(2026 - 09 - 15)).await,
        Some(date!(2026 - 09 - 15))
    );

    // Through the service, what the company was charged beyond its people
    // counts for the company alone.
    let service =
        AiAdoptionServiceImpl::new(Arc::new(repository(&pool)), stockholm(), Some(credits()));
    let adoption = service
        .adoption(Some(date!(2026 - 08 - 01)), Some(date!(2026 - 10 - 31)))
        .await
        .unwrap();
    assert_eq!(
        adoption.company_days,
        vec![AiAdoptionCompanyDay {
            day: date!(2026 - 10 - 05),
            tool: AiProvider::Copilot,
            amount: AiUsdNanos::new(45).unwrap(),
        }]
    );
    let costs: Vec<_> = adoption
        .days
        .iter()
        .map(|day| {
            (
                day.user_id,
                day.day,
                day.tool,
                day.usage.priced_cost.nanos(),
            )
        })
        .collect();
    assert_eq!(
        costs,
        vec![
            (first, date!(2026 - 09 - 15), AiProvider::Copilot, 100),
            (second, date!(2026 - 10 - 05), AiProvider::Copilot, 55),
        ]
    );
}

#[sqlx::test]
async fn everyone_linked_is_listed_and_anyone_else_with_use(pool: PgPool) {
    let linked = insert_user(&pool, "linked@example.com", "User").await;
    let active = insert_user(&pool, "active@example.com", "User").await;
    let idle = insert_user(&pool, "idle@example.com", "User").await;
    sqlx::query(
        "INSERT INTO time_tracking_provider_users
            (provider, provider_company_id, provider_user_id, name, active)
         VALUES ('kleer', 'company-1', 'p-1', 'Linked', true)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO time_tracking_user_links
            (user_id, provider, provider_company_id, provider_user_id)
         VALUES ($1, 'kleer', 'company-1', 'p-1')",
    )
    .bind(linked.as_i32())
    .execute(&pool)
    .await
    .unwrap();
    record(
        &pool,
        active,
        MACHINE_B,
        vec![bucket(datetime!(2026-10-05 08:00 UTC), APP, 1, Some(0.1))],
    )
    .await;
    insert_account(&pool, 21).await;
    dispose(&pool, 21, Some(idle), date!(2020 - 01 - 01), None).await;
    sqlx::query(
        "INSERT INTO ai_provider_seats (provider, account_id, seat_created_at, first_listed_at,
            last_listed_at)
         VALUES ('copilot', 21, '2025-03-04 23:30:00+00', now(), now())",
    )
    .execute(&pool)
    .await
    .unwrap();

    let people = repository(&pool)
        .people(date!(2026 - 10 - 28), &stockholm())
        .await
        .unwrap();
    let idle_person = people.iter().find(|person| person.user_id == idle).unwrap();
    // Assigned late on 4 March UTC: 5 March in Stockholm.
    assert_eq!(
        idle_person.seats,
        vec![AiAdoptionSeat {
            provider: AiProvider::Copilot,
            since: date!(2025 - 03 - 05),
        }]
    );
    assert!(people
        .iter()
        .any(|person| person.user_id == linked && person.linked));
    assert!(people
        .iter()
        .any(|person| person.user_id == active && person.first_synced_on.is_some()));

    let service = AiAdoptionServiceImpl::new(Arc::new(repository(&pool)), stockholm(), None);
    let listed: Vec<UserId> = service
        .adoption(Some(date!(2026 - 10 - 01)), Some(date!(2026 - 10 - 31)))
        .await
        .unwrap()
        .people
        .into_iter()
        .map(|person| person.user_id)
        .collect();
    assert!(listed.contains(&linked));
    assert!(listed.contains(&active));
    assert!(!listed.contains(&idle));
}
