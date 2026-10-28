use sqlx::PgPool;
use time::macros::{date, datetime};

use super::*;
use crate::{
    adapters::outbound::postgres::ai_usage_fixtures::{bucket, insert_user, record},
    domain::models::{AiListedSeat, AiUsageBucket},
};

const COPILOT: AiProvider = AiProvider::Copilot;
const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

fn repository(pool: &PgPool) -> PostgresAiProviderAccountRepository {
    PostgresAiProviderAccountRepository::new(DbPool::from(pool.clone()))
}

fn account(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn seat(id: i64, login: &str, created_at: OffsetDateTime) -> AiListedSeat {
    AiListedSeat {
        account_id: account(id),
        login: AiProviderLogin::parse(login).unwrap(),
        created_at,
        pending_cancellation_on: None,
        last_activity_at: Some(datetime!(2026-10-24 10:05:06 UTC)),
        last_activity_client: Some("copilot-cli".to_string()),
    }
}

fn listing(seats: Vec<AiListedSeat>) -> AiSeatListing {
    AiSeatListing {
        reported_total: Some(u32::try_from(seats.len()).unwrap()),
        seats,
        ended_on_short_page: true,
    }
}

fn ada() -> AiListedSeat {
    seat(1001, "ada-lovelace", datetime!(2023-04-17 07:34:52 +02:00))
}

fn grace() -> AiListedSeat {
    seat(1002, "grace-hopper", datetime!(2025-01-31 08:57:49 +01:00))
}

async fn stored(pool: &PgPool) -> HashMap<i64, AiStoredProviderAccount> {
    repository(pool)
        .accounts(COPILOT)
        .await
        .unwrap()
        .into_iter()
        .map(|account| (account.account_id.as_i64(), account))
        .collect()
}

fn terms(
    target: AiAccountTarget,
    valid_from: Date,
    valid_to: Option<Date>,
) -> AiAccountDispositionTerms {
    AiAccountDispositionTerms {
        target,
        period: AiAccountPeriod::new(valid_from, valid_to).unwrap(),
    }
}

#[sqlx::test]
async fn a_seat_listing_stores_each_assignment_once(pool: PgPool) {
    let repository = repository(&pool);

    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
            .await
            .unwrap(),
        (2, 0)
    );
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
            .await
            .unwrap(),
        (0, 0)
    );

    // GitHub renders the same instants in another offset once DST ends.
    let rerendered = vec![
        AiListedSeat {
            created_at: datetime!(2023-04-17 06:34:52 +01:00),
            ..ada()
        },
        AiListedSeat {
            created_at: datetime!(2025-01-31 07:57:49 UTC),
            ..grace()
        },
    ];
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(rerendered))
            .await
            .unwrap(),
        (0, 0)
    );

    let accounts = stored(&pool).await;
    assert_eq!(accounts.len(), 2);
    let ada_seats = &accounts[&1001].seats;
    assert_eq!(ada_seats.len(), 1);
    assert_eq!(ada_seats[0].created_at, datetime!(2023-04-17 05:34:52 UTC));
    assert_eq!(
        ada_seats[0].last_activity_client.as_deref(),
        Some("copilot-cli")
    );
    assert!(ada_seats[0].is_listed());
    assert!(ada_seats[0].first_listed_at <= ada_seats[0].last_listed_at);
    assert_eq!(
        repository.account_ids(COPILOT).await.unwrap(),
        [account(1001), account(1002)]
    );
}

#[sqlx::test]
async fn listing_a_seat_again_keeps_the_latest_cancellation_and_activity(pool: PgPool) {
    let repository = repository(&pool);
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada()]))
        .await
        .unwrap();
    let first = stored(&pool).await[&1001].seats[0].clone();

    // The same assignment with a cancellation announced and newer activity.
    let cancelling = AiListedSeat {
        pending_cancellation_on: Some(date!(2026 - 10 - 31)),
        last_activity_at: Some(datetime!(2026-10-25 14:00 UTC)),
        last_activity_client: Some("vscode/1.143.0/copilot-chat/0.71.0".to_string()),
        ..ada()
    };
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![cancelling.clone()]))
            .await
            .unwrap(),
        (0, 0)
    );
    let seats = &stored(&pool).await[&1001].seats;
    assert_eq!(seats.len(), 1);
    let relisted = &seats[0];
    assert_eq!(
        relisted.pending_cancellation_on,
        Some(date!(2026 - 10 - 31))
    );
    assert_eq!(
        relisted.last_activity_at,
        Some(datetime!(2026-10-25 14:00 UTC))
    );
    assert_eq!(
        relisted.last_activity_client.as_deref(),
        Some("vscode/1.143.0/copilot-chat/0.71.0")
    );
    assert_eq!(relisted.first_listed_at, first.first_listed_at);
    assert!(relisted.last_listed_at > first.last_listed_at);

    // A withdrawn cancellation is no longer pending.
    let withdrawn = AiListedSeat {
        pending_cancellation_on: None,
        ..cancelling
    };
    repository
        .store_seat_listing(COPILOT, &listing(vec![withdrawn]))
        .await
        .unwrap();
    assert_eq!(
        stored(&pool).await[&1001].seats[0].pending_cancellation_on,
        None
    );
}

#[sqlx::test]
async fn only_a_complete_listing_ends_seats_it_omits(pool: PgPool) {
    let repository = repository(&pool);
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
        .await
        .unwrap();

    // GitHub reported two seats but one was listed: nothing ends.
    let incomplete = AiSeatListing {
        reported_total: Some(2),
        ..listing(vec![ada()])
    };
    assert!(!incomplete.is_complete());
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &incomplete)
            .await
            .unwrap(),
        (0, 0)
    );
    assert!(stored(&pool).await[&1002].seats[0].is_listed());

    // A complete listing without Grace ends her seat and keeps her account.
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![ada()]))
            .await
            .unwrap(),
        (0, 1)
    );
    let accounts = stored(&pool).await;
    assert!(!accounts[&1002].seats[0].is_listed());
    assert!(accounts[&1001].seats[0].is_listed());

    // Listed again with the same assignment: reopened, not a new seat.
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
            .await
            .unwrap(),
        (0, 0)
    );
    let grace_seats = &stored(&pool).await[&1002].seats;
    assert_eq!(grace_seats.len(), 1);
    assert!(grace_seats[0].is_listed());
}

#[sqlx::test]
async fn a_new_assignment_ends_the_old_seat_and_a_rename_follows(pool: PgPool) {
    let repository = repository(&pool);
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
        .await
        .unwrap();

    let reassigned = AiListedSeat {
        login: AiProviderLogin::parse("countess-ada").unwrap(),
        created_at: datetime!(2026-10-16 09:00 UTC),
        pending_cancellation_on: Some(date!(2026 - 11 - 30)),
        last_activity_at: None,
        last_activity_client: None,
        ..ada()
    };
    assert_eq!(
        repository
            .store_seat_listing(COPILOT, &listing(vec![reassigned, grace()]))
            .await
            .unwrap(),
        (1, 1)
    );

    let accounts = stored(&pool).await;
    let ada = &accounts[&1001];
    assert_eq!(ada.login.as_str(), "countess-ada");
    assert_eq!(ada.seats.len(), 2);
    assert!(!ada.seats[0].is_listed());
    assert!(ada.seats[1].is_listed());
    assert_eq!(
        ada.seats[1].pending_cancellation_on,
        Some(date!(2026 - 11 - 30))
    );
    assert_eq!(ada.seats[1].last_activity_client, None);
}

#[sqlx::test]
async fn profiles_and_refresh_status_are_stored(pool: PgPool) {
    let repository = repository(&pool);
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
        .await
        .unwrap();
    assert!(stored(&pool).await[&1001].profile.is_none());

    repository
        .store_profiles(
            COPILOT,
            &[
                AiProviderAccountProfile {
                    account_id: account(1001),
                    login: AiProviderLogin::parse("ada-renamed").unwrap(),
                    name: Some("Ada Lovelace".to_string()),
                    email: Some("ada@example.com".to_string()),
                },
                // Unknown accounts are ignored.
                AiProviderAccountProfile {
                    account_id: account(9999),
                    login: AiProviderLogin::parse("stranger").unwrap(),
                    name: None,
                    email: None,
                },
            ],
        )
        .await
        .unwrap();
    let accounts = stored(&pool).await;
    let ada = &accounts[&1001];
    assert_eq!(ada.login.as_str(), "ada-renamed");
    let profile = ada.profile.as_ref().unwrap();
    assert_eq!(profile.name.as_deref(), Some("Ada Lovelace"));
    assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
    assert_eq!(accounts.len(), 2);

    assert_eq!(repository.directory_status(COPILOT).await.unwrap(), None);
    repository
        .record_success(COPILOT, 34, Some(34))
        .await
        .unwrap();
    let success = repository.directory_status(COPILOT).await.unwrap().unwrap();
    assert_eq!(success.last_success_at, Some(success.last_attempt_at));
    assert_eq!(
        (success.seats_listed, success.seats_reported),
        (Some(34), Some(34))
    );
    assert_eq!(success.last_error, None);

    // A failure keeps the last success and its counts.
    repository
        .record_failure(COPILOT, "the account directory is unavailable: synthetic")
        .await
        .unwrap();
    let failure = repository.directory_status(COPILOT).await.unwrap().unwrap();
    assert_eq!(failure.last_success_at, success.last_success_at);
    assert_eq!(failure.seats_listed, Some(34));
    assert!(failure.last_attempt_at >= success.last_attempt_at);
    assert_eq!(
        failure.last_error.as_deref(),
        Some("the account directory is unavailable: synthetic")
    );

    repository
        .record_success(COPILOT, 33, Some(34))
        .await
        .unwrap();
    let recovered = repository.directory_status(COPILOT).await.unwrap().unwrap();
    assert_eq!(recovered.last_error, None);
    assert_eq!(recovered.seats_listed, Some(33));
}

#[sqlx::test]
async fn dispositions_never_overlap_per_account_or_per_user(pool: PgPool) {
    let repository = repository(&pool);
    let admin = insert_user(&pool, "admin@example.com", "Admin").await;
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada(), grace()]))
        .await
        .unwrap();
    let october = terms(
        AiAccountTarget::User(dev),
        date!(2026 - 10 - 01),
        Some(date!(2026 - 10 - 31)),
    );

    let first = repository
        .insert_disposition(COPILOT, account(1001), &october, &admin)
        .await
        .unwrap();
    assert_eq!(first.login.as_str(), "ada-lovelace");
    assert_eq!(first.terms, october);
    assert_eq!(first.updated_by, Some(admin));

    // The same account on a shared day.
    let shared = terms(AiAccountTarget::External, date!(2026 - 10 - 31), None);
    assert!(matches!(
        repository.insert_disposition(COPILOT, account(1001), &shared, &admin).await,
        Err(AiProviderAccountError::AccountOverlap { login }) if login == "ada-lovelace"
    ));
    // The same user through another account, naming the first.
    assert!(matches!(
        repository
            .insert_disposition(
                COPILOT,
                account(1002),
                &terms(AiAccountTarget::User(dev), date!(2026 - 10 - 15), None),
                &admin,
            )
            .await,
        Err(AiProviderAccountError::UserOverlap { login }) if login == "ada-lovelace"
    ));

    // The next day is free, for the account and for the user.
    repository
        .insert_disposition(
            COPILOT,
            account(1001),
            &terms(AiAccountTarget::External, date!(2026 - 11 - 01), None),
            &admin,
        )
        .await
        .unwrap();
    let grace_from_november = repository
        .insert_disposition(
            COPILOT,
            account(1002),
            &terms(AiAccountTarget::User(dev), date!(2026 - 11 - 01), None),
            &admin,
        )
        .await
        .unwrap();
    // Externals never conflict with each other across accounts.
    repository
        .insert_disposition(
            COPILOT,
            account(1002),
            &terms(
                AiAccountTarget::External,
                date!(2026 - 01 - 01),
                Some(date!(2026 - 10 - 31)),
            ),
            &admin,
        )
        .await
        .unwrap();

    // An ongoing disposition conflicts with every later start.
    assert!(matches!(
        repository
            .insert_disposition(
                COPILOT,
                account(1002),
                &terms(AiAccountTarget::External, date!(2030 - 01 - 01), None),
                &admin,
            )
            .await,
        Err(AiProviderAccountError::AccountOverlap { .. })
    ));
    // An update is checked the same way, ignoring itself.
    assert!(matches!(
        repository
            .update_disposition(
                COPILOT,
                grace_from_november.id,
                &terms(AiAccountTarget::User(dev), date!(2026 - 10 - 20), None),
                &admin,
            )
            .await,
        Err(AiProviderAccountError::AccountOverlap { login }) if login == "grace-hopper"
    ));
    let ended = repository
        .update_disposition(
            COPILOT,
            grace_from_november.id,
            &terms(
                AiAccountTarget::User(dev),
                date!(2026 - 11 - 01),
                Some(date!(2026 - 11 - 30)),
            ),
            &admin,
        )
        .await
        .unwrap();
    assert_eq!(ended.terms.period.valid_to(), Some(date!(2026 - 11 - 30)));

    assert_eq!(stored(&pool).await[&1002].dispositions.len(), 2);
}

#[sqlx::test]
async fn disposition_writes_report_what_is_missing(pool: PgPool) {
    let repository = repository(&pool);
    let admin = insert_user(&pool, "admin@example.com", "Admin").await;
    repository
        .store_seat_listing(COPILOT, &listing(vec![ada()]))
        .await
        .unwrap();
    let external = terms(AiAccountTarget::External, date!(2026 - 10 - 01), None);

    assert!(matches!(
        repository
            .insert_disposition(
                COPILOT,
                account(1001),
                &terms(
                    AiAccountTarget::User(UserId::new(424242)),
                    date!(2026 - 10 - 01),
                    None
                ),
                &admin,
            )
            .await,
        Err(AiProviderAccountError::UserNotFound)
    ));
    assert!(matches!(
        repository
            .insert_disposition(COPILOT, account(4040), &external, &admin)
            .await,
        Err(AiProviderAccountError::AccountNotFound)
    ));
    let missing = AiAccountDispositionId::new(424242);
    assert!(matches!(
        repository
            .update_disposition(COPILOT, missing, &external, &admin)
            .await,
        Err(AiProviderAccountError::NotFound)
    ));
    assert!(matches!(
        repository.delete_disposition(COPILOT, missing).await,
        Err(AiProviderAccountError::NotFound)
    ));

    // An update writes the account's current login.
    let created = repository
        .insert_disposition(COPILOT, account(1001), &external, &admin)
        .await
        .unwrap();
    repository
        .store_seat_listing(
            COPILOT,
            &listing(vec![AiListedSeat {
                login: AiProviderLogin::parse("countess-ada").unwrap(),
                ..ada()
            }]),
        )
        .await
        .unwrap();
    let updated = repository
        .update_disposition(COPILOT, created.id, &external, &admin)
        .await
        .unwrap();
    assert_eq!(updated.login.as_str(), "countess-ada");

    repository
        .delete_disposition(COPILOT, created.id)
        .await
        .unwrap();
    assert!(stored(&pool).await[&1001].dispositions.is_empty());
}

fn paid(
    hour_start: OffsetDateTime,
    provider: AiProvider,
    billing_provider: Option<AiProvider>,
    cost: f64,
) -> AiUsageBucket {
    AiUsageBucket {
        provider,
        billing_provider,
        model: "gpt-6.1-sol".to_string(),
        ..bucket(hour_start, "github.com/example/app", 2, Some(cost))
    }
}

#[sqlx::test]
async fn metered_usage_is_what_the_payer_paid_for_per_utc_day(pool: PgPool) {
    let dev = insert_user(&pool, "dev@example.com", "User").await;
    record(
        &pool,
        dev,
        MACHINE,
        vec![
            paid(
                datetime!(2026-10-31 22:00 UTC),
                AiProvider::Copilot,
                None,
                1.25,
            ),
            // 23:00 UTC on Oct 31 is Nov 1 in Stockholm, but Oct 31 for GitHub.
            paid(
                datetime!(2026-10-31 23:00 UTC),
                AiProvider::OpenCode,
                Some(AiProvider::Copilot),
                0.75,
            ),
            // Copilot routed through a ChatGPT sign-in is Codex's.
            paid(
                datetime!(2026-10-31 23:00 UTC),
                AiProvider::Copilot,
                Some(AiProvider::Codex),
                40.0,
            ),
            paid(
                datetime!(2026-11-01 00:00 UTC),
                AiProvider::Copilot,
                None,
                2.0,
            ),
            paid(
                datetime!(2026-10-30 12:00 UTC),
                AiProvider::Claude,
                None,
                9.0,
            ),
        ],
    )
    .await;
    // Personal usage counts too: GitHub bills it all the same.
    sqlx::query("INSERT INTO ai_personal_project_keys (user_id, project_key) VALUES ($1, $2)")
        .bind(dev.as_i32())
        .bind("github.com/example/app")
        .execute(&pool)
        .await
        .unwrap();

    let days = repository(&pool)
        .metered_usage(
            COPILOT,
            AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), date!(2026 - 10 - 31)).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(days.len(), 1);
    assert_eq!((days[0].user_id, days[0].day), (dev, date!(2026 - 10 - 31)));
    assert_eq!(days[0].usage.priced_cost.nanos(), 2_000_000_000);
    assert_eq!(days[0].usage.records, 4);
    assert_eq!(days[0].usage.tokens.input, 40);
}
