use time::{
    macros::{date, datetime},
    OffsetDateTime,
};

use super::*;
use crate::domain::models::{
    AiAccountDisposition, AiAccountDispositionTerms, AiAccountTarget, AiProviderLogin,
    AiStoredProfile, AiStoredSeat,
};

const TODAY: Date = date!(2026 - 10 - 24);
const ADA: UserId = UserId::new(1);
const ALAN: UserId = UserId::new(2);
const GRACE: UserId = UserId::new(3);

fn users() -> Vec<AiBillingDeveloper> {
    [
        (ADA, "Ada Lovelace"),
        (ALAN, "Alan Turing"),
        (GRACE, "Grace Hopper"),
    ]
    .into_iter()
    .map(|(user_id, full_name)| AiBillingDeveloper {
        user_id,
        full_name: full_name.to_string(),
        email: format!("{}@example.com", full_name.to_lowercase().replace(' ', ".")),
    })
    .collect()
}

fn seat(
    created_at: OffsetDateTime,
    ended: Option<(OffsetDateTime, OffsetDateTime)>,
) -> AiStoredSeat {
    AiStoredSeat {
        created_at,
        pending_cancellation_on: None,
        last_activity_at: None,
        last_activity_client: None,
        first_listed_at: created_at,
        last_listed_at: ended.map_or(datetime!(2026-10-24 06:00 UTC), |(last, _)| last),
        unlisted_at: ended.map(|(_, unlisted)| unlisted),
    }
}

fn account(
    id: i64,
    login: &str,
    name: Option<&str>,
    seats: Vec<AiStoredSeat>,
    dispositions: Vec<(i32, AiAccountTarget, Date, Option<Date>)>,
) -> AiStoredProviderAccount {
    let account_id = AiProviderAccountId::new(id).unwrap();
    let login = AiProviderLogin::parse(login).unwrap();
    AiStoredProviderAccount {
        account_id,
        profile: Some(AiStoredProfile {
            name: name.map(str::to_string),
            email: None,
            fetched_at: datetime!(2026-10-24 06:00 UTC),
        }),
        seats,
        dispositions: dispositions
            .into_iter()
            .map(
                |(disposition_id, target, valid_from, valid_to)| AiAccountDisposition {
                    id: AiAccountDispositionId::new(disposition_id),
                    payer: AiProvider::Copilot,
                    account_id,
                    login: login.clone(),
                    terms: AiAccountDispositionTerms {
                        target,
                        period: AiAccountPeriod::new(valid_from, valid_to).unwrap(),
                    },
                    updated_at: datetime!(2026-10-24 06:00 UTC),
                    updated_by: None,
                },
            )
            .collect(),
        login,
    }
}

fn metered(user_id: UserId, day: Date, cents: i64) -> AiMeteredUserDay {
    AiMeteredUserDay {
        user_id,
        day,
        usage: AiBillingUsage {
            records: 3,
            priced_cost: AiUsdNanos::new(cents * 10_000_000).unwrap(),
            ..AiBillingUsage::default()
        },
    }
}

fn overview(
    accounts: Vec<AiStoredProviderAccount>,
    metered: Vec<AiMeteredUserDay>,
) -> AiAccountOverview {
    overview_with_credits(accounts, metered, Vec::new())
}

fn overview_with_credits(
    accounts: Vec<AiStoredProviderAccount>,
    metered: Vec<AiMeteredUserDay>,
    credits: Vec<AiAccountCreditDay>,
) -> AiAccountOverview {
    account_overview(AiAccountOverviewInput {
        today: TODAY,
        review_from: review_from(TODAY),
        accounts,
        users: users(),
        metered,
        namespace: "Example-AB".to_string(),
        credits,
        credits_month: AiBillingMonth::containing(TODAY).unwrap(),
    })
    .unwrap()
}

/// `credits` whole credits of account `id` on `day`.
fn credit_day(id: i64, day: Date, credits: i64) -> AiAccountCreditDay {
    AiAccountCreditDay {
        account_id: AiProviderAccountId::new(id).unwrap(),
        day,
        gross: AiUsdNanos::new(credits * 10_000_000).unwrap(),
    }
}

fn row<'a>(overview: &'a AiAccountOverview, login: &str) -> &'a AiAccountRow {
    overview
        .accounts
        .iter()
        .find(|row| row.account.login.as_str() == login)
        .unwrap()
}

fn uncovered(row: &AiAccountRow) -> Vec<(Date, Date)> {
    row.uncovered_seat_days
        .iter()
        .map(|range| (range.start(), range.last_day()))
        .collect()
}

#[test]
fn the_review_window_starts_with_last_utc_month() {
    assert_eq!(review_from(TODAY), date!(2026 - 09 - 01));
    assert_eq!(review_from(date!(2027 - 01 - 15)), date!(2026 - 12 - 01));
    assert_eq!(review_from(date!(2026 - 03 - 31)), date!(2026 - 02 - 01));
}

#[test]
fn accounts_need_a_look_for_seated_days_without_a_disposition() {
    let held = seat(datetime!(2025-03-14 15:40 +01:00), None);
    let overview = overview(
        vec![
            account(1, "nobody-yet", None, vec![held.clone()], vec![]),
            account(
                2,
                "late-start",
                None,
                vec![held.clone()],
                vec![(1, AiAccountTarget::User(ADA), date!(2026 - 10 - 16), None)],
            ),
            // Alan's link started late and has ended.
            account(
                3,
                "ended-link",
                None,
                vec![held.clone()],
                vec![(
                    2,
                    AiAccountTarget::User(ALAN),
                    date!(2025 - 06 - 01),
                    Some(date!(2026 - 10 - 15)),
                )],
            ),
            account(
                4,
                "external",
                None,
                vec![held],
                vec![(3, AiAccountTarget::External, date!(2025 - 01 - 01), None)],
            ),
            // Its seat ended before the window.
            account(
                5,
                "long-gone",
                None,
                vec![seat(
                    datetime!(2025-01-01 08:00 UTC),
                    Some((
                        datetime!(2026-06-30 06:00 UTC),
                        datetime!(2026-07-01 06:00 UTC),
                    )),
                )],
                vec![],
            ),
        ],
        vec![],
    );

    let nobody = row(&overview, "nobody-yet");
    assert!(nobody.needs_look);
    assert_eq!(uncovered(nobody), [(date!(2026 - 09 - 01), TODAY)]);
    assert_eq!(nobody.current_disposition, None);

    let late = row(&overview, "late-start");
    assert_eq!(
        uncovered(late),
        [(date!(2026 - 09 - 01), date!(2026 - 10 - 15))]
    );
    assert_eq!(
        late.current_disposition,
        Some(AiAccountDispositionId::new(1))
    );

    // The days before Alan's link are uncovered too, but a new disposition
    // defaults to the flagged ones.
    let ended = row(&overview, "ended-link");
    assert_eq!(uncovered(ended), [(date!(2026 - 10 - 16), TODAY)]);
    assert_eq!(ended.current_disposition, None);
    assert_eq!(
        ended.default_period,
        Some(AiAccountPeriod::new(date!(2026 - 10 - 16), None).unwrap())
    );

    let external = row(&overview, "external");
    assert!(!external.needs_look);
    assert_eq!(
        external.current_disposition,
        Some(AiAccountDispositionId::new(3))
    );
    assert_eq!(external.default_period, None);

    assert!(!row(&overview, "long-gone").needs_look);
}

#[test]
fn users_with_metered_usage_on_unlinked_days_are_listed() {
    let held = seat(datetime!(2026-01-01 08:00 UTC), None);
    let overview = overview(
        vec![
            // Ada is linked from Oct 10 only.
            account(
                1,
                "ada-gh",
                Some("Ada Lovelace"),
                vec![held.clone()],
                vec![(1, AiAccountTarget::User(ADA), date!(2026 - 10 - 10), None)],
            ),
            // Alan's account has no disposition yet; its top suggestion is Alan.
            account(2, "alan-gh", Some("Alan Turing"), vec![held], vec![]),
        ],
        vec![
            metered(ADA, date!(2026 - 10 - 08), 100),
            metered(ADA, date!(2026 - 10 - 09), 50),
            metered(ADA, date!(2026 - 10 - 12), 900),
            metered(ALAN, date!(2026 - 09 - 30), 20),
            metered(ALAN, date!(2026 - 10 - 20), 30),
            // Outside the window.
            metered(GRACE, date!(2026 - 08 - 31), 10_000),
        ],
    );

    let unlinked: Vec<_> = overview
        .unlinked_users
        .iter()
        .map(|user| {
            (
                user.user_id,
                user.uncovered_days,
                user.first_day,
                user.last_day,
                user.usage.priced_cost.nanos(),
                user.suggested_accounts.clone(),
            )
        })
        .collect();
    assert_eq!(
        unlinked,
        [
            (
                ADA,
                2,
                date!(2026 - 10 - 08),
                date!(2026 - 10 - 09),
                1_500_000_000,
                // The gap before her link is suggested to be hers too.
                vec![AiProviderAccountId::new(1).unwrap()],
            ),
            (
                ALAN,
                2,
                date!(2026 - 09 - 30),
                date!(2026 - 10 - 20),
                500_000_000,
                vec![AiProviderAccountId::new(2).unwrap()],
            ),
        ]
    );

    // Activity covers the whole window, linked days included.
    let activity: HashMap<_, _> = overview.activity.iter().cloned().collect();
    assert_eq!(activity[&ADA].days, 3);
    assert_eq!(activity[&ADA].last_day, Some(date!(2026 - 10 - 12)));
    assert_eq!(activity[&ADA].usage.priced_cost.nanos(), 10_500_000_000);
    assert!(!activity.contains_key(&GRACE));
}

#[test]
fn accounts_are_ordered_by_need_then_activity_then_login() {
    let held = seat(datetime!(2026-01-01 08:00 UTC), None);
    let overview = overview(
        vec![
            account(
                1,
                "covered",
                None,
                vec![held.clone()],
                vec![(1, AiAccountTarget::External, date!(2025 - 01 - 01), None)],
            ),
            account(2, "zed", None, vec![held.clone()], vec![]),
            account(3, "Alpha", None, vec![held.clone()], vec![]),
            account(
                4,
                "grace-gh",
                Some("Grace Hopper"),
                vec![held.clone()],
                vec![],
            ),
            account(5, "alan-gh", Some("Alan Turing"), vec![held], vec![]),
        ],
        vec![
            metered(GRACE, date!(2026 - 10 - 01), 100),
            metered(ALAN, date!(2026 - 10 - 01), 900),
        ],
    );

    assert_eq!(
        overview
            .accounts
            .iter()
            .map(|row| row.account.login.as_str())
            .collect::<Vec<_>>(),
        ["alan-gh", "grace-gh", "Alpha", "zed", "covered"]
    );
    assert_eq!(
        overview
            .unlinked_users
            .iter()
            .map(|user| user.user_id)
            .collect::<Vec<_>>(),
        [ALAN, GRACE]
    );
}

#[test]
fn credits_on_days_without_a_disposition_need_a_look() {
    // Ada holds a seat since September and is linked from October 10; Grace
    // never held a seat but used credits; Alan is linked throughout.
    let ada = account(
        1001,
        "ada-lovelace",
        Some("Ada Lovelace"),
        vec![seat(datetime!(2026-09-01 08:00 UTC), None)],
        vec![(1, AiAccountTarget::User(ADA), date!(2026 - 10 - 10), None)],
    );
    let grace = account(1002, "grace-hopper", None, Vec::new(), Vec::new());
    let alan = account(
        1003,
        "alan-turing",
        None,
        vec![seat(datetime!(2026-08-01 08:00 UTC), None)],
        vec![(2, AiAccountTarget::User(ALAN), date!(2026 - 08 - 01), None)],
    );
    let overview = overview_with_credits(
        vec![ada, grace, alan],
        Vec::new(),
        vec![
            // Before the review window: never flagged, not this month.
            credit_day(1001, date!(2026 - 08 - 20), 500),
            credit_day(1001, date!(2026 - 10 - 08), 100),
            credit_day(1001, date!(2026 - 10 - 09), 50),
            credit_day(1001, date!(2026 - 10 - 12), 25),
            credit_day(1002, date!(2026 - 09 - 30), 10),
            credit_day(1002, date!(2026 - 10 - 01), 20),
            credit_day(1003, date!(2026 - 10 - 02), 0),
            credit_day(1003, date!(2026 - 10 - 03), 300),
        ],
    );
    assert_eq!(overview.credits_month.to_string(), "2026-10");

    let ada = row(&overview, "ada-lovelace");
    assert_eq!(ada.credits_this_month.nanos(), 175 * 10_000_000);
    let ranges: Vec<(Date, Date)> = ada
        .uncovered_credit_days
        .iter()
        .map(|range| (range.start(), range.last_day()))
        .collect();
    assert_eq!(ranges, [(date!(2026 - 10 - 08), date!(2026 - 10 - 09))]);
    assert_eq!(ada.credit_days_without_seat, 0);
    assert!(ada.needs_look);

    // Credits without any seat: every such day lacks a seat and needs a look.
    let grace = row(&overview, "grace-hopper");
    assert_eq!(grace.credits_this_month.nanos(), 20 * 10_000_000);
    assert_eq!(grace.uncovered_credit_days.len(), 1);
    assert_eq!(grace.credit_days_without_seat, 2);
    assert!(grace.uncovered_seat_days.is_empty());
    assert!(grace.needs_look);

    let alan = row(&overview, "alan-turing");
    assert_eq!(alan.credits_this_month.nanos(), 300 * 10_000_000);
    assert!(alan.uncovered_credit_days.is_empty());
    assert!(!alan.needs_look);
}

#[test]
fn credit_days_alone_give_a_default_period() {
    // Alan's seat, from October 1, is linked throughout, but he used credits
    // in September under a seat Toki never listed. Grace never held a seat.
    let alan = account(
        1003,
        "alan-turing",
        Some("Alan Turing"),
        vec![seat(datetime!(2026-10-01 08:00 UTC), None)],
        vec![(2, AiAccountTarget::User(ALAN), date!(2026 - 10 - 01), None)],
    );
    let grace = account(
        1002,
        "grace-hopper",
        Some("Grace Hopper"),
        Vec::new(),
        Vec::new(),
    );
    let overview = overview_with_credits(
        vec![alan, grace],
        Vec::new(),
        vec![
            credit_day(1003, date!(2026 - 09 - 20), 30),
            credit_day(1003, date!(2026 - 09 - 22), 10),
            credit_day(1002, date!(2026 - 09 - 30), 10),
            credit_day(1002, date!(2026 - 10 - 01), 20),
        ],
    );
    let period = |from: Date, to: Date| Some(AiAccountPeriod::new(from, Some(to)).unwrap());

    // Through the day before his disposition, so it can be set or suggested.
    let alan = row(&overview, "alan-turing");
    assert!(alan.uncovered_seat_days.is_empty());
    assert!(alan.needs_look);
    assert_eq!(
        alan.default_period,
        period(date!(2026 - 09 - 20), date!(2026 - 09 - 30))
    );
    assert_eq!(alan.suggestions.first().map(|s| s.user_id), Some(ALAN));

    // Without a seat: the credit days, not today onwards.
    let grace = row(&overview, "grace-hopper");
    assert_eq!(
        grace.default_period,
        period(date!(2026 - 09 - 30), date!(2026 - 10 - 01))
    );
    assert_eq!(grace.suggestions.first().map(|s| s.user_id), Some(GRACE));
}
