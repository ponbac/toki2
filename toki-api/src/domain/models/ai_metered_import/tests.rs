use time::macros::{date, datetime};

use super::*;

const BACKFILL: Date = date!(2026 - 09 - 01);
const ADMIN: UserId = UserId::new(7);

fn month(raw: &str) -> AiBillingMonth {
    AiBillingMonth::parse(raw).unwrap()
}

fn account(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn plan_months(plan: &AiImportPlan) -> Vec<(String, AiMonthMode)> {
    plan.months
        .iter()
        .map(|(month, mode)| (month.to_string(), *mode))
        .collect()
}

fn history(attempt: Option<OffsetDateTime>, nightly: Option<OffsetDateTime>) -> AiImportHistory {
    AiImportHistory {
        last_attempt_at: attempt,
        last_success_at: attempt,
        last_nightly_success_at: nightly,
    }
}

#[test]
fn the_first_run_backfills_every_month_in_changes_mode() {
    // Before 02:00 UTC with nothing imported: an hourly run whose gaps are
    // September and October.
    let plan = scheduled_plan(
        datetime!(2026-10-25 01:30 UTC),
        BACKFILL,
        &AiImportHistory::default(),
        &[month("2026-09"), month("2026-10")],
        DEFAULT_SETTLE_DAYS,
    )
    .unwrap();
    assert_eq!(plan.kind, AiImportKind::Hourly);
    assert_eq!(
        plan_months(&plan),
        [
            ("2026-09".to_string(), AiMonthMode::Changes),
            ("2026-10".to_string(), AiMonthMode::Changes),
        ]
    );
}

#[test]
fn the_nightly_refresh_is_due_from_two_utc_until_one_succeeds() {
    let ran = Some(datetime!(2026-10-25 00:50 UTC));
    // 01:59: hourly; 02:00: nightly.
    let before = scheduled_plan(
        datetime!(2026-10-25 01:59 UTC),
        BACKFILL,
        &history(ran, Some(datetime!(2026-10-24 02:00 UTC))),
        &[],
        DEFAULT_SETTLE_DAYS,
    )
    .unwrap();
    assert_eq!(before.kind, AiImportKind::Hourly);
    assert_eq!(
        plan_months(&before),
        [("2026-10".to_string(), AiMonthMode::Changes)]
    );

    let at = scheduled_plan(
        datetime!(2026-10-25 02:00 UTC),
        BACKFILL,
        &history(ran, Some(datetime!(2026-10-24 02:00 UTC))),
        &[],
        DEFAULT_SETTLE_DAYS,
    )
    .unwrap();
    assert_eq!(at.kind, AiImportKind::Nightly);
    // The previous month ended 25 days ago: still refreshed in full.
    assert_eq!(
        plan_months(&at),
        [
            ("2026-09".to_string(), AiMonthMode::Full),
            ("2026-10".to_string(), AiMonthMode::Full),
        ]
    );

    // A nightly run already succeeded today.
    let done = scheduled_plan(
        datetime!(2026-10-25 05:00 UTC),
        BACKFILL,
        &history(
            Some(datetime!(2026-10-25 02:00 UTC)),
            Some(datetime!(2026-10-25 02:00 UTC)),
        ),
        &[],
        DEFAULT_SETTLE_DAYS,
    )
    .unwrap();
    assert_eq!(done.kind, AiImportKind::Hourly);
}

#[test]
fn a_run_is_due_55_minutes_after_the_last_attempt() {
    let last = datetime!(2026-10-25 10:00 UTC);
    let nightly = Some(datetime!(2026-10-25 02:00 UTC));
    assert!(scheduled_plan(
        last + Duration::minutes(54),
        BACKFILL,
        &history(Some(last), nightly),
        &[],
        DEFAULT_SETTLE_DAYS
    )
    .is_none());
    assert!(scheduled_plan(
        last + Duration::minutes(55),
        BACKFILL,
        &history(Some(last), nightly),
        &[],
        DEFAULT_SETTLE_DAYS
    )
    .is_some());
}

#[test]
fn the_previous_month_is_refreshed_nightly_for_30_days_and_hourly_while_settling() {
    let nightly_plan = |now: OffsetDateTime| {
        scheduled_plan(now, BACKFILL, &AiImportHistory::default(), &[], 3).unwrap()
    };
    // September ended on the 30th: October 30 is day 30, October 31 day 31.
    assert_eq!(
        plan_months(&nightly_plan(datetime!(2026-10-30 03:00 UTC))).len(),
        2
    );
    assert_eq!(
        plan_months(&nightly_plan(datetime!(2026-10-31 03:00 UTC))),
        [("2026-10".to_string(), AiMonthMode::Full)]
    );

    let hourly = |now: OffsetDateTime, settle_days: u32| {
        scheduled_plan(
            now,
            BACKFILL,
            &history(None, Some(now - Duration::hours(1))),
            &[],
            settle_days,
        )
        .unwrap()
    };
    assert_eq!(
        plan_months(&hourly(datetime!(2026-10-03 10:00 UTC), 3)),
        [
            ("2026-09".to_string(), AiMonthMode::Changes),
            ("2026-10".to_string(), AiMonthMode::Changes),
        ]
    );
    assert_eq!(
        plan_months(&hourly(datetime!(2026-10-04 10:00 UTC), 3)).len(),
        1
    );
    // A longer observed lag keeps it longer.
    assert_eq!(
        plan_months(&hourly(datetime!(2026-10-04 10:00 UTC), 4)).len(),
        2
    );
}

#[test]
fn gap_months_join_in_changes_mode_and_nothing_precedes_the_backfill() {
    let plan = scheduled_plan(
        datetime!(2026-12-10 03:00 UTC),
        date!(2026 - 10 - 15),
        &AiImportHistory::default(),
        &[month("2026-09"), month("2026-10")],
        3,
    )
    .unwrap();
    assert_eq!(plan.kind, AiImportKind::Nightly);
    assert_eq!(
        plan_months(&plan),
        [
            ("2026-10".to_string(), AiMonthMode::Changes),
            ("2026-11".to_string(), AiMonthMode::Full),
            ("2026-12".to_string(), AiMonthMode::Full),
        ]
    );
    // Nothing before the backfill day.
    assert!(scheduled_plan(
        datetime!(2026-08-10 03:00 UTC),
        BACKFILL,
        &AiImportHistory::default(),
        &[],
        3
    )
    .is_none());
}

#[test]
fn admin_plans_stay_within_the_imported_months() {
    let today = date!(2026 - 10 - 25);
    let plan = admin_plan(
        month("2026-09"),
        AiMonthMode::Full,
        today,
        BACKFILL,
        &[month("2026-10")],
        ADMIN,
    )
    .unwrap();
    assert_eq!(plan.kind, AiImportKind::Admin(Some(ADMIN)));
    assert_eq!(
        plan_months(&plan),
        [
            ("2026-09".to_string(), AiMonthMode::Full),
            ("2026-10".to_string(), AiMonthMode::Changes),
        ]
    );
    for outside in ["2026-08", "2026-11"] {
        let error = admin_plan(
            month(outside),
            AiMonthMode::Changes,
            today,
            BACKFILL,
            &[],
            ADMIN,
        )
        .unwrap_err();
        assert!(
            matches!(&error, AiMeteredUsageError::Invalid(message) if message.contains(outside)),
            "{error}"
        );
    }
}

#[test]
fn open_days_are_the_settle_window_within_the_month() {
    let october = month("2026-10");
    assert_eq!(
        open_days(october, date!(2026 - 10 - 25), 3),
        [
            date!(2026 - 10 - 22),
            date!(2026 - 10 - 23),
            date!(2026 - 10 - 24),
            date!(2026 - 10 - 25),
        ]
    );
    assert_eq!(
        open_days(october, date!(2026 - 10 - 02), 3),
        [date!(2026 - 10 - 01), date!(2026 - 10 - 02)]
    );
    assert_eq!(
        open_days(month("2026-09"), date!(2026 - 10 - 02), 3),
        [date!(2026 - 09 - 29), date!(2026 - 09 - 30)]
    );
    assert!(open_days(month("2026-09"), date!(2026 - 10 - 25), 3).is_empty());
}

fn slice(
    source: AiMeteredSource,
    account_id: Option<i64>,
    day: Date,
    complete: bool,
) -> AiSliceRecord {
    let fetched_at = day.next_day().unwrap().midnight().assume_utc() + Duration::hours(1);
    AiSliceRecord {
        source,
        account_id: account_id.map(account),
        day,
        fetched_at,
        changed_at: fetched_at,
        complete_at: complete.then_some(fetched_at),
        late_changed_at: None,
    }
}

/// Every slice of `accounts` and the organization from `from` through
/// `through`, complete unless listed in `incomplete`.
fn slices(
    accounts: &[i64],
    from: Date,
    through: Date,
    incomplete: &[(Option<i64>, Date)],
) -> Vec<AiSliceRecord> {
    let mut records = Vec::new();
    let mut day = from;
    while day <= through {
        for id in accounts {
            let complete = !incomplete.contains(&(Some(*id), day));
            records.push(slice(
                AiMeteredSource::AccountCredits,
                Some(*id),
                day,
                complete,
            ));
        }
        for source in [AiMeteredSource::OrgCredits, AiMeteredSource::OrgUsage] {
            records.push(slice(source, None, day, !incomplete.contains(&(None, day))));
        }
        if day.day() == 1 {
            records.push(slice(AiMeteredSource::Summary, None, day, false));
        }
        day = day.next_day().unwrap();
    }
    records
}

#[test]
fn coverage_follows_the_expected_slices() {
    let today = date!(2026 - 10 - 10);
    let from = date!(2026 - 10 - 01);
    let expected = AiExpectedSlices::new(&[account(1), account(2)], from, today);
    let records = slices(&[1, 2], from, date!(2026 - 10 - 09), &[]);
    let complete = AiSliceSet::complete(&records);
    assert_eq!(
        imported_through(AiMeteredSource::AccountCredits, &expected, &complete, today),
        Some(date!(2026 - 10 - 09))
    );
    assert!(gap_months(&expected, &AiSliceSet::fetched(&records)).is_empty());

    // A day not yet complete stops coverage there.
    let records = slices(
        &[1, 2],
        from,
        date!(2026 - 10 - 09),
        &[(Some(2), date!(2026 - 10 - 05))],
    );
    let complete = AiSliceSet::complete(&records);
    assert_eq!(
        imported_through(AiMeteredSource::AccountCredits, &expected, &complete, today),
        Some(date!(2026 - 10 - 04))
    );
    assert_eq!(
        imported_through(AiMeteredSource::OrgCredits, &expected, &complete, today),
        Some(date!(2026 - 10 - 09))
    );

    // An account listed later is expected from the backfill day: its missing
    // days are a gap.
    let expected = AiExpectedSlices::new(&[account(1), account(2), account(3)], from, today);
    let records = slices(&[1, 2], from, date!(2026 - 10 - 09), &[]);
    assert_eq!(
        imported_through(
            AiMeteredSource::AccountCredits,
            &expected,
            &AiSliceSet::complete(&records),
            today
        ),
        None
    );
    assert_eq!(
        gap_months(&expected, &AiSliceSet::fetched(&records)),
        [month("2026-10")]
    );

    // A missing September day is a September gap; today is never a gap.
    let expected = AiExpectedSlices::new(&[account(1)], date!(2026 - 09 - 29), today);
    let records = slices(&[1], date!(2026 - 09 - 30), date!(2026 - 10 - 09), &[]);
    assert_eq!(
        gap_months(&expected, &AiSliceSet::fetched(&records)),
        [month("2026-09")]
    );
}

#[test]
fn the_settle_lag_counts_changes_after_a_complete_fetch() {
    let today = date!(2026 - 10 - 25);
    let mut late = slice(
        AiMeteredSource::AccountCredits,
        Some(1),
        date!(2026 - 10 - 20),
        true,
    );
    // Found changed 1 day and 3 hours after the day ended: 2 days.
    late.late_changed_at = Some(datetime!(2026-10-22 03:00 UTC));
    let unchanged = slice(
        AiMeteredSource::AccountCredits,
        Some(2),
        date!(2026 - 10 - 20),
        true,
    );
    assert_eq!(
        settle_lag_days(&[late.clone(), unchanged.clone()], today),
        Some(2)
    );
    assert_eq!(settle_lag_days(&[unchanged], today), None);
    // Outside the 62-day window.
    assert_eq!(settle_lag_days(&[late], date!(2026 - 12 - 31)), None);
    assert_eq!(settle_days(Some(2)), DEFAULT_SETTLE_DAYS);
    assert_eq!(settle_days(Some(5)), 5);
}

fn run(status: AiRunStatus, error: Option<&str>) -> AiImportRun {
    AiImportRun {
        id: AiImportRunId::new(1),
        kind: AiImportKind::Hourly,
        months: vec![(month("2026-10"), AiMonthMode::Changes)],
        started_at: datetime!(2026-10-25 09:00 UTC),
        finished_at: Some(datetime!(2026-10-25 09:05 UTC)),
        status,
        requests: 40,
        slices_fetched: 10,
        slices_changed: 0,
        account_errors: 0,
        error: error.map(str::to_string),
        sources: Vec::new(),
    }
}

fn fresh_input() -> AiFreshnessInput {
    AiFreshnessInput {
        source_ready: true,
        now: datetime!(2026-10-25 10:00 UTC),
        history: AiImportHistory {
            last_attempt_at: Some(datetime!(2026-10-25 09:00 UTC)),
            last_success_at: Some(datetime!(2026-10-25 09:05 UTC)),
            last_nightly_success_at: Some(datetime!(2026-10-25 02:00 UTC)),
        },
        latest_run: Some(run(AiRunStatus::Succeeded, None)),
        imported_through: vec![
            (AiMeteredSource::AccountCredits, Some(date!(2026 - 10 - 24))),
            (AiMeteredSource::OrgCredits, Some(date!(2026 - 10 - 23))),
            (AiMeteredSource::OrgUsage, Some(date!(2026 - 10 - 24))),
            (AiMeteredSource::Summary, Some(date!(2026 - 09 - 30))),
        ],
        summary_fetched_at: Some(datetime!(2026-10-25 09:01 UTC)),
        account_errors: Vec::new(),
    }
}

fn stale_reason(input: &AiFreshnessInput) -> AiStaleReason {
    match import_freshness(input) {
        AiImportFreshness::Stale { reason } => reason,
        other => panic!("not stale: {other:?}"),
    }
}

fn account_error(seated: bool) -> AiAccountImportError {
    AiAccountImportError {
        account_id: account(1001),
        login: AiProviderLogin::parse("ada-lovelace").unwrap(),
        month: month("2026-10"),
        error: "not found".to_string(),
        seated,
    }
}

#[test]
fn freshness_names_the_first_reason_it_is_stale() {
    assert_eq!(import_freshness(&fresh_input()), AiImportFreshness::Fresh);
    assert_eq!(
        import_freshness(&AiFreshnessInput {
            source_ready: false,
            ..fresh_input()
        }),
        AiImportFreshness::Off
    );
    assert_eq!(
        import_freshness(&AiFreshnessInput {
            latest_run: None,
            ..fresh_input()
        }),
        AiImportFreshness::Never
    );

    let failed = AiFreshnessInput {
        latest_run: Some(run(AiRunStatus::Failed, Some("GitHub is down"))),
        ..fresh_input()
    };
    assert_eq!(
        stale_reason(&failed),
        AiStaleReason::LastRunFailed {
            error: "GitHub is down".to_string()
        }
    );
    // The log line reads as a clause.
    assert_eq!(
        stale_reason(&failed).to_string(),
        "the last import failed: GitHub is down"
    );

    let old_success = AiFreshnessInput {
        now: datetime!(2026-10-25 12:06 UTC),
        ..fresh_input()
    };
    let last_success = fresh_input().history.last_success_at.unwrap();
    assert_eq!(
        stale_reason(&old_success),
        AiStaleReason::LastSuccessOld { at: last_success }
    );
    assert!(stale_reason(&old_success)
        .to_string()
        .starts_with("the last successful import was at 2026-10-25T"));

    let mut behind = fresh_input();
    behind.imported_through[0].1 = Some(date!(2026 - 10 - 20));
    assert_eq!(
        stale_reason(&behind),
        AiStaleReason::SourceBehind {
            source: AiMeteredSource::AccountCredits,
            through: date!(2026 - 10 - 20)
        }
    );
    assert_eq!(
        stale_reason(&behind).to_string(),
        "account credits imported through 2026-10-20"
    );
    behind.imported_through[0].1 = None;
    assert_eq!(
        stale_reason(&behind),
        AiStaleReason::SourceNotImported {
            source: AiMeteredSource::AccountCredits
        }
    );

    let old_summary = AiFreshnessInput {
        summary_fetched_at: Some(datetime!(2026-10-25 06:00 UTC)),
        ..fresh_input()
    };
    assert_eq!(
        stale_reason(&old_summary),
        AiStaleReason::SummaryOld {
            at: datetime!(2026-10-25 06:00 UTC)
        }
    );

    let seated = AiFreshnessInput {
        account_errors: vec![account_error(true)],
        ..fresh_input()
    };
    assert_eq!(
        stale_reason(&seated),
        AiStaleReason::AccountFailed {
            login: AiProviderLogin::parse("ada-lovelace").unwrap(),
            month: month("2026-10"),
            error: "not found".to_string()
        }
    );
    assert_eq!(
        stale_reason(&seated).to_string(),
        "ada-lovelace could not be imported for 2026-10 (not found)"
    );
    // An account without a seat that month is listed, never stale.
    let departed = AiFreshnessInput {
        account_errors: vec![account_error(false)],
        latest_run: Some(run(AiRunStatus::Partial, None)),
        ..fresh_input()
    };
    assert_eq!(import_freshness(&departed), AiImportFreshness::Fresh);
}

#[test]
fn kinds_and_statuses_round_trip_their_names() {
    for kind in [
        AiImportKind::Hourly,
        AiImportKind::Nightly,
        AiImportKind::Admin(Some(ADMIN)),
    ] {
        assert_eq!(
            AiImportKind::parse(kind.as_str(), kind.requested_by()),
            Some(kind)
        );
    }
    for status in [
        AiRunStatus::Running,
        AiRunStatus::Succeeded,
        AiRunStatus::Partial,
        AiRunStatus::Failed,
    ] {
        assert_eq!(AiRunStatus::parse(status.as_str()), Some(status));
    }
    for status in [
        AiSourceStatus::Succeeded,
        AiSourceStatus::Partial,
        AiSourceStatus::Failed,
        AiSourceStatus::Skipped,
    ] {
        assert_eq!(AiSourceStatus::parse(status.as_str()), Some(status));
    }
}
