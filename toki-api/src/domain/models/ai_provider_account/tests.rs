use time::macros::{date, datetime};

use super::*;

const ADA: UserId = UserId::new(7);
const TODAY: Date = date!(2026 - 10 - 24);

fn account_id(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn period(valid_from: Date, valid_to: Option<Date>) -> AiAccountPeriod {
    AiAccountPeriod::new(valid_from, valid_to).unwrap()
}

/// A seat assigned at `created_at` and still listed.
fn listed_seat(created_at: OffsetDateTime) -> AiStoredSeat {
    AiStoredSeat {
        created_at,
        pending_cancellation_on: None,
        last_activity_at: None,
        last_activity_client: None,
        first_listed_at: datetime!(2026-10-20 06:00 UTC),
        last_listed_at: datetime!(2026-10-24 06:00 UTC),
        unlisted_at: None,
    }
}

/// A seat last listed at `last_listed_at` and found unlisted at `unlisted_at`.
fn ended_seat(
    created_at: OffsetDateTime,
    last_listed_at: OffsetDateTime,
    unlisted_at: OffsetDateTime,
) -> AiStoredSeat {
    AiStoredSeat {
        first_listed_at: created_at,
        last_listed_at,
        unlisted_at: Some(unlisted_at),
        ..listed_seat(created_at)
    }
}

fn disposition(
    id: i32,
    target: AiAccountTarget,
    valid_from: Date,
    valid_to: Option<Date>,
) -> AiAccountDisposition {
    AiAccountDisposition {
        id: AiAccountDispositionId::new(id),
        payer: AiProvider::Copilot,
        account_id: account_id(1001),
        login: AiProviderLogin::parse("ada-lovelace").unwrap(),
        terms: AiAccountDispositionTerms {
            target,
            period: period(valid_from, valid_to),
        },
        updated_at: datetime!(2026-10-24 08:00 UTC),
        updated_by: None,
    }
}

fn account(
    seats: Vec<AiStoredSeat>,
    dispositions: Vec<AiAccountDisposition>,
) -> AiStoredProviderAccount {
    AiStoredProviderAccount {
        account_id: account_id(1001),
        login: AiProviderLogin::parse("ada-lovelace").unwrap(),
        profile: None,
        seats,
        dispositions,
    }
}

fn days(ranges: &[AiUsageDateRange]) -> Vec<(Date, Date)> {
    ranges
        .iter()
        .map(|range| (range.start(), range.last_day()))
        .collect()
}

#[test]
fn account_ids_are_positive_and_logins_have_no_whitespace() {
    assert!(AiProviderAccountId::new(0).is_none());
    assert!(AiProviderAccountId::new(-3).is_none());
    assert_eq!(account_id(89738831).as_i64(), 89738831);

    assert_eq!(
        AiProviderLogin::parse(" ada-lovelace ").unwrap().as_str(),
        "ada-lovelace"
    );
    for bad in ["", "   ", "ada lovelace", "ada\u{7}", &"a".repeat(101)] {
        assert!(AiProviderLogin::parse(bad).is_err(), "{bad:?}");
    }
    assert!(AiProviderLogin::parse(&"a".repeat(100)).is_ok());
}

#[test]
fn free_text_is_trimmed_clipped_and_dropped_when_empty() {
    assert_eq!(clip_text(Some("  Ada  "), 512).as_deref(), Some("Ada"));
    assert_eq!(clip_text(Some("   "), 512), None);
    assert_eq!(clip_text(None, 512), None);
    // Characters, not bytes.
    assert_eq!(clip_text(Some("Åsa Ängström"), 3).as_deref(), Some("Åsa"));
    assert_eq!(clip_text(Some(&"x".repeat(600)), 512).unwrap().len(), 512);
}

#[test]
fn a_period_covers_both_ends_and_an_ongoing_one_has_no_end() {
    let october = period(date!(2026 - 10 - 01), Some(date!(2026 - 10 - 31)));
    assert!(!october.contains(date!(2026 - 09 - 30)));
    assert!(october.contains(date!(2026 - 10 - 01)));
    assert!(october.contains(date!(2026 - 10 - 31)));
    assert!(!october.contains(date!(2026 - 11 - 01)));

    let ongoing = period(date!(2026 - 11 - 01), None);
    assert!(ongoing.contains(date!(2099 - 01 - 01)));
    // Adjacent periods share no day; an ongoing one meets every later one.
    assert!(!october.overlaps(&ongoing));
    assert!(ongoing.overlaps(&period(date!(2027 - 01 - 01), Some(date!(2027 - 01 - 01)))));
    assert!(october.overlaps(&period(date!(2026 - 10 - 31), None)));

    // One day; to_date_range is half-open.
    let one_day = period(TODAY, Some(TODAY));
    let within = AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), TODAY).unwrap();
    let range = one_day.to_date_range(within).unwrap();
    assert_eq!((range.start(), range.end()), (TODAY, date!(2026 - 10 - 25)));
    assert_eq!(
        period(date!(2026 - 11 - 01), None).to_date_range(within),
        None
    );
}

#[test]
fn a_period_needs_ordered_dates_with_four_digit_years() {
    assert!(AiAccountPeriod::new(date!(2026 - 10 - 02), Some(date!(2026 - 10 - 01))).is_err());
    assert!(AiAccountPeriod::new(date!(0000 - 12 - 31), None).is_err());
    assert!(AiAccountPeriod::new(date!(0001 - 01 - 01), Some(date!(9999 - 12 - 31))).is_ok());
}

#[test]
fn a_listing_is_complete_only_when_it_ended_on_a_short_page_with_every_reported_seat() {
    let seat = |id| AiListedSeat {
        account_id: account_id(id),
        login: AiProviderLogin::parse(&format!("user-{id}")).unwrap(),
        created_at: datetime!(2026-01-01 08:00 UTC),
        pending_cancellation_on: None,
        last_activity_at: None,
        last_activity_client: None,
    };
    let listing = |count: i64, reported: Option<u32>, short: bool| AiSeatListing {
        seats: (1..=count).map(seat).collect(),
        reported_total: reported,
        ended_on_short_page: short,
    };

    assert!(listing(3, Some(3), true).is_complete());
    // More listed than reported, as with seats pending cancellation.
    assert!(listing(4, Some(3), true).is_complete());
    assert!(!listing(2, Some(3), true).is_complete());
    // Pagination that did not end on a short page may have more.
    assert!(!listing(3, Some(3), false).is_complete());
    assert!(!listing(3, None, true).is_complete());
    // An organization without seats.
    assert!(listing(0, Some(0), true).is_complete());
}

#[test]
fn a_seat_ends_on_its_pending_cancellation_day_or_the_day_it_was_last_listed() {
    let created = datetime!(2026-10-16 09:30 +02:00);
    let listed = listed_seat(created);
    assert_eq!(listed.first_day(), date!(2026 - 10 - 16));
    assert_eq!(listed.last_day(), None);

    // A cancellation GitHub announced, found unlisted the next morning.
    let cancelled = AiStoredSeat {
        pending_cancellation_on: Some(date!(2026 - 10 - 31)),
        ..ended_seat(
            created,
            datetime!(2026-10-30 06:00 UTC),
            datetime!(2026-11-01 06:00 UTC),
        )
    };
    assert_eq!(cancelled.last_day(), Some(date!(2026 - 10 - 31)));

    // Removed without notice: the day it was last listed.
    let removed = ended_seat(
        created,
        datetime!(2026-10-20 06:00 UTC),
        datetime!(2026-10-21 06:00 UTC),
    );
    assert_eq!(removed.last_day(), Some(date!(2026 - 10 - 20)));

    // A pending day outside the gap says nothing about this removal.
    let stale_pending = AiStoredSeat {
        pending_cancellation_on: Some(date!(2026 - 12 - 31)),
        ..removed.clone()
    };
    assert_eq!(stale_pending.last_day(), Some(date!(2026 - 10 - 20)));
}

#[test]
fn the_first_day_is_the_utc_day_of_the_assignment_instant() {
    // 00:30 in Stockholm is still the previous UTC day.
    let seat = listed_seat(datetime!(2026-10-16 00:30 +02:00));
    assert_eq!(seat.first_day(), date!(2026 - 10 - 15));
}

#[test]
fn seated_days_merge_seats_and_run_through_today_while_listed() {
    let held = account(
        vec![
            ended_seat(
                datetime!(2025-01-31 08:57 +01:00),
                datetime!(2025-06-30 06:00 UTC),
                datetime!(2025-07-01 06:00 UTC),
            ),
            listed_seat(datetime!(2026-10-16 09:00 UTC)),
        ],
        vec![],
    );
    assert_eq!(
        days(&held.seated_days(TODAY)),
        [
            (date!(2025 - 01 - 31), date!(2025 - 06 - 30)),
            (date!(2026 - 10 - 16), TODAY),
        ]
    );
    assert!(account(vec![], vec![]).seated_days(TODAY).is_empty());
}

#[test]
fn uncovered_seat_days_leave_out_days_with_any_disposition() {
    let within = AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), TODAY).unwrap();
    let seat = listed_seat(datetime!(2026-08-25 10:00 UTC));

    let unlinked = account(vec![seat.clone()], vec![]);
    assert_eq!(
        days(&unlinked.uncovered_seat_days(TODAY, within)),
        [(date!(2026 - 09 - 01), TODAY)]
    );

    // A late start, then an External that ended: two gaps.
    let gappy = account(
        vec![seat.clone()],
        vec![
            disposition(
                1,
                AiAccountTarget::External,
                date!(2026 - 09 - 10),
                Some(date!(2026 - 09 - 30)),
            ),
            disposition(2, AiAccountTarget::User(ADA), date!(2026 - 10 - 16), None),
        ],
    );
    assert_eq!(
        days(&gappy.uncovered_seat_days(TODAY, within)),
        [
            (date!(2026 - 09 - 01), date!(2026 - 09 - 09)),
            (date!(2026 - 10 - 01), date!(2026 - 10 - 15)),
        ]
    );

    // A seat that ended before the window needs nothing.
    let old = account(
        vec![ended_seat(
            datetime!(2026-01-01 08:00 UTC),
            datetime!(2026-07-31 06:00 UTC),
            datetime!(2026-08-01 06:00 UTC),
        )],
        vec![],
    );
    assert!(old.uncovered_seat_days(TODAY, within).is_empty());
}

#[test]
fn a_new_disposition_defaults_to_the_days_that_need_one() {
    let within = AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), TODAY).unwrap();
    let seat = listed_seat(datetime!(2023-04-17 05:34 UTC));
    let default_period = |account: &AiStoredProviderAccount| account.default_period(TODAY, within);

    // No dispositions: from the first seat day, ongoing.
    assert_eq!(
        default_period(&account(vec![seat.clone()], vec![])),
        Some(period(date!(2023 - 04 - 17), None))
    );

    // A disposition that starts late leaves the gap before it.
    let late = account(
        vec![seat.clone()],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2026 - 10 - 16),
            None,
        )],
    );
    assert_eq!(
        default_period(&late),
        Some(period(date!(2023 - 04 - 17), Some(date!(2026 - 10 - 15))))
    );

    // An ended link while the seat is still held: from the next day.
    let ended = account(
        vec![seat.clone()],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2023 - 04 - 17),
            Some(date!(2026 - 10 - 15)),
        )],
    );
    assert_eq!(
        default_period(&ended),
        Some(period(date!(2026 - 10 - 16), None))
    );

    // A link that ends after today: from the day after it, for the seat that
    // is still listed then.
    let ending = account(
        vec![seat.clone()],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2023 - 04 - 17),
            Some(date!(2026 - 12 - 31)),
        )],
    );
    assert_eq!(
        default_period(&ending),
        Some(period(date!(2027 - 01 - 01), None))
    );

    // Ongoing coverage of every seated day: nothing to set.
    let covered = account(
        vec![seat],
        vec![disposition(
            1,
            AiAccountTarget::External,
            date!(2023 - 01 - 01),
            None,
        )],
    );
    assert_eq!(default_period(&covered), None);

    // No seats at all: today, ongoing.
    assert_eq!(
        default_period(&account(vec![], vec![])),
        Some(period(TODAY, None))
    );
}

#[test]
fn a_new_disposition_defaults_to_the_flagged_gap_over_an_earlier_one() {
    let within = AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), TODAY).unwrap();
    // Held since 2024; Ada's link started late and ended before today.
    let late_and_ended = account(
        vec![listed_seat(datetime!(2024-03-01 09:00 UTC))],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2025 - 01 - 01),
            Some(date!(2026 - 10 - 15)),
        )],
    );
    assert_eq!(
        days(&late_and_ended.uncovered_seat_days(TODAY, within)),
        [(date!(2026 - 10 - 16), TODAY)]
    );
    assert_eq!(
        late_and_ended.default_period(TODAY, within),
        Some(period(date!(2026 - 10 - 16), None))
    );

    // With nothing flagged, the earlier gap, through the day before the link.
    let late = account(
        vec![listed_seat(datetime!(2024-03-01 09:00 UTC))],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2025 - 01 - 01),
            None,
        )],
    );
    assert_eq!(
        late.default_period(TODAY, within),
        Some(period(date!(2024 - 03 - 01), Some(date!(2024 - 12 - 31))))
    );

    // A seat gap no disposition separates joins the flagged stretch.
    let returned = account(
        vec![
            ended_seat(
                datetime!(2025-01-31 08:57 UTC),
                datetime!(2025-06-30 06:00 UTC),
                datetime!(2025-07-01 06:00 UTC),
            ),
            listed_seat(datetime!(2026-10-16 09:00 UTC)),
        ],
        vec![disposition(
            1,
            AiAccountTarget::External,
            date!(2025 - 01 - 01),
            Some(date!(2025 - 03 - 31)),
        )],
    );
    assert_eq!(
        returned.default_period(TODAY, within),
        Some(period(date!(2025 - 04 - 01), None))
    );
}

#[test]
fn an_account_whose_seats_ended_defaults_through_its_last_seated_day() {
    let within = AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), TODAY).unwrap();
    let held = ended_seat(
        datetime!(2026-01-01 08:00 UTC),
        datetime!(2026-06-30 06:00 UTC),
        datetime!(2026-07-01 06:00 UTC),
    );

    assert_eq!(
        account(vec![held.clone()], vec![]).default_period(TODAY, within),
        Some(period(date!(2026 - 01 - 01), Some(date!(2026 - 06 - 30))))
    );

    // External until Ada joined, then nobody: Ada's stretch.
    let external_first = account(
        vec![held.clone()],
        vec![disposition(
            1,
            AiAccountTarget::External,
            date!(2026 - 01 - 01),
            Some(date!(2026 - 03 - 31)),
        )],
    );
    assert_eq!(
        external_first.default_period(TODAY, within),
        Some(period(date!(2026 - 04 - 01), Some(date!(2026 - 06 - 30))))
    );

    // A link that ended with the seat leaves nothing to set, so no ongoing
    // period after the seat claims the user.
    let past = account(
        vec![held],
        vec![disposition(
            1,
            AiAccountTarget::User(ADA),
            date!(2026 - 01 - 01),
            Some(date!(2026 - 06 - 30)),
        )],
    );
    assert_eq!(past.default_period(TODAY, within), None);
}

#[test]
fn a_refresh_is_due_once_a_utc_day_and_retried_hourly_after_failures() {
    let now = datetime!(2026-10-24 10:00 UTC);
    let status = |last_attempt_at, last_success_at| AiDirectoryStatus {
        last_attempt_at,
        last_success_at,
        last_error: None,
        seats_listed: None,
        seats_reported: None,
    };

    // Never refreshed.
    assert!(directory_refresh_due(None, now));
    // Succeeded today, even just after midnight.
    let today = datetime!(2026-10-24 00:05 UTC);
    assert!(!directory_refresh_due(
        Some(&status(today, Some(today))),
        now
    ));
    // Succeeded yesterday.
    let yesterday = datetime!(2026-10-23 09:00 UTC);
    assert!(directory_refresh_due(
        Some(&status(yesterday, Some(yesterday))),
        now
    ));
    // Failed 10 minutes ago: wait.
    assert!(!directory_refresh_due(
        Some(&status(datetime!(2026-10-24 09:50 UTC), Some(yesterday))),
        now
    ));
    // Failed 2 hours ago: retry.
    assert!(directory_refresh_due(
        Some(&status(datetime!(2026-10-24 08:00 UTC), Some(yesterday))),
        now
    ));
    // Exactly the retry delay counts as due.
    assert!(directory_refresh_due(
        Some(&status(now - DIRECTORY_RETRY_AFTER, None)),
        now
    ));
}
