use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{
    ai_metered_fixtures::*, reconcile, split_group, AiAccountTarget, AiStoredProviderAccount,
    AiTrackedModelDay, AiUsageCheck,
};

const WIDGET: &str = "Example Client - Widget API";
const PORTAL: &str = "Other Client - Portal";

/// A line as `(subject, kind, project, Unassigned kind or SKU, credits)`.
type LineSummary = (String, &'static str, String, i64);

fn line(subject: &str, kind: &'static str, target: &str, credits: i64) -> LineSummary {
    (subject.to_string(), kind, target.to_string(), credits)
}

fn summary(allocation: &AiMeteredAllocation) -> Vec<LineSummary> {
    allocation
        .lines
        .iter()
        .map(|line| {
            let subject = match &line.subject {
                AiBillingSubject::User(user) => format!("user {}", user.as_i32()),
                AiBillingSubject::ProviderAccount { login, .. } => format!("account {login}"),
                AiBillingSubject::UnknownAccount { .. } => "unknown account".to_string(),
                AiBillingSubject::Repository { name, .. } => format!("repository {name}"),
            };
            let kind = match line.charge.kind {
                AiMeteredChargeKind::Project => "project",
                AiMeteredChargeKind::Personal => "personal",
                AiMeteredChargeKind::Untracked => "untracked",
                AiMeteredChargeKind::Account(AiMeteredAccountReason::Unlinked) => "unlinked",
                AiMeteredChargeKind::Account(AiMeteredAccountReason::External) => "external",
                AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown) => "unknown",
                AiMeteredChargeKind::Repository => "repository",
            };
            let target = match (&line.project, line.unassigned) {
                (Some(project), _) => project.name.clone(),
                (None, Some(AiUnassignedKind::Unmapped)) => "unmapped".to_string(),
                (None, Some(AiUnassignedKind::Unattributed)) => "unattributed".to_string(),
                (None, None) => line.charge.sku.clone().unwrap_or_default(),
            };
            (subject, kind, target, line.charge.credits())
        })
        .collect()
}

/// Allocates and asserts the invariants on the lines the bill gets, against
/// the reconciliation tab's accounts: each account's lines sum to its gross
/// less code review, rounded once; their nanodollars are its attributed
/// (project and personal lines), untracked, unlinked and external credits,
/// and their usage what it tracked; no project or personal line bills more
/// than its tracked cost.
fn allocate(input: &AiReconciliationInput) -> AiMeteredAllocation {
    use AiMeteredAccountReason::{External, Unlinked};
    use AiMeteredChargeKind::{Account, Personal, Project, Untracked};
    let allocation = allocate_metered_credits(input).unwrap();
    let reconciliation = reconcile(input.clone()).unwrap();
    for line in &allocation.lines {
        if let Some(billed) = &line.charge.account {
            assert!(
                reconciliation
                    .accounts
                    .iter()
                    .any(|account| account.account_id == billed.account_id),
                "{line:?}"
            );
        }
    }
    for account in &reconciliation.accounts {
        let lines: Vec<&AiMeteredLine> = allocation
            .lines
            .iter()
            .filter(|line| {
                line.charge
                    .account
                    .as_ref()
                    .is_some_and(|billed| billed.account_id == account.account_id)
            })
            .collect();
        let allocated = |kinds: &[AiMeteredChargeKind]| -> i64 {
            lines
                .iter()
                .filter(|line| kinds.contains(&line.charge.kind))
                .map(|line| line.charge.allocated.nanos())
                .sum()
        };
        let tracked: i64 = lines
            .iter()
            .filter(|line| matches!(line.charge.kind, Project | Personal))
            .map(|line| line.usage.priced_cost.nanos())
            .sum();
        let credits: i64 = lines.iter().map(|line| line.charge.credits()).sum();
        assert_eq!(
            credits,
            usd(account.gross.nanos() - account.repository_billed.nanos()).to_cents(),
            "{}",
            account.login
        );
        assert_eq!(
            (
                allocated(&[Project, Personal]),
                allocated(&[Untracked]),
                allocated(&[Account(Unlinked)]),
                allocated(&[Account(External)]),
                tracked,
            ),
            (
                account.attributed.nanos(),
                account.untracked.nanos(),
                account.unlinked.nanos(),
                account.external.nanos(),
                account.tracked.nanos(),
            ),
            "{}",
            account.login
        );
    }
    for line in &allocation.lines {
        assert_eq!(line.charge.amount.currency, AiCurrency::usd());
        if matches!(line.charge.kind, Project | Personal) {
            assert!(
                line.charge.credits() * CREDIT <= line.usage.priced_cost.nanos(),
                "{line:?}"
            );
            assert!(line.charge.allocated <= line.usage.priced_cost, "{line:?}");
        }
    }
    allocation
}

fn reasons(allocation: &AiMeteredAllocation, user: i32) -> Vec<AiMeteredProvisional> {
    let lines: Vec<&AiMeteredLine> = allocation
        .lines
        .iter()
        .filter(|line| line.subject == AiBillingSubject::User(UserId::new(user)))
        .collect();
    assert!(!lines.is_empty(), "user {user} has no line");
    lines[0].charge.provisional.clone()
}

#[test]
fn the_cutover_starts_at_midnight_utc_of_its_month() {
    let cutover = AiMeteredCutover {
        payer: AiProvider::Copilot,
        from: october(),
    };
    assert_eq!(cutover.starts_at(), datetime!(2026-10-01 00:00 UTC));
    assert!(!cutover.applies_to(AiBillingMonth::parse("2026-09").unwrap()));
    assert!(cutover.applies_to(october()));
    assert!(cutover.applies_to(AiBillingMonth::parse("2027-01").unwrap()));
}

#[test]
fn a_group_splits_gross_by_tracked_cost_and_never_charges_more_than_tracked() {
    // G = 0 bills nothing, however much was tracked.
    assert_eq!(split_group(0, &[5, 5]).unwrap(), (vec![0, 0], 0));
    // Nothing tracked, or only unpriced usage: all of G is untracked.
    assert_eq!(split_group(100, &[]).unwrap(), (vec![], 100));
    assert_eq!(split_group(100, &[0]).unwrap(), (vec![0], 100));
    // T = G and T ≫ G: G split by T's weights, nothing untracked.
    assert_eq!(split_group(100, &[60, 40]).unwrap(), (vec![60, 40], 0));
    assert_eq!(split_group(10, &[300, 100]).unwrap(), (vec![8, 2], 0));
    // Ties go to the earlier destination.
    assert_eq!(split_group(1, &[1, 1]).unwrap(), (vec![1, 0], 0));
    // T < G: each destination its own T, the rest untracked.
    assert_eq!(split_group(100, &[30, 20]).unwrap(), (vec![30, 20], 50));
    assert!(split_group(-1, &[1]).is_err());
}

#[test]
fn a_fully_tracked_user_bills_gross_by_each_projects_tracked_cost() {
    let day = date!(2026 - 10 - 05);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "100")];
    input.tracked = vec![
        tracked_to(ADA, day, "gpt-6.1-sol", to_project(widget()), 75 * CREDIT),
        tracked_to(ADA, day, "gpt-6.1-sol", to_project(portal()), 50 * CREDIT),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 60),
            line("user 1", "project", PORTAL, 40),
        ]
    );
    let widget_line = &allocation.lines[0];
    assert_eq!(widget_line.charge.allocated, usd(60 * CREDIT));
    assert_eq!(widget_line.charge.sku.as_deref(), Some(CREDITS_SKU));
    assert_eq!(
        widget_line.charge.account,
        Some(AiMeteredAccountRef {
            account_id: account_id(1001),
            login: AiProviderLogin::parse("ada-lovelace").unwrap(),
        })
    );
    assert_eq!(widget_line.usage.priced_cost, usd(75 * CREDIT));
    assert!(allocation
        .lines
        .iter()
        .all(|line| line.charge.provisional.is_empty()));
    assert!(!allocation.provisional);
    assert_eq!(allocation.unbilled, AiMeteredUnbilled::default());
    // G/T is 0.8: split as always, and flagged for a look.
    assert!(allocation.lines.iter().all(|line| line.charge.low_ratio));
    assert_eq!(allocation.low_ratio_groups, 1);
}

#[test]
fn a_partly_untracked_user_bills_each_destination_its_tracked_cost_and_the_rest_untracked() {
    let day = date!(2026 - 10 - 06);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "200")];
    input.tracked = vec![
        tracked_to(ADA, day, "gpt-6.1-sol", to_project(widget()), 120 * CREDIT),
        tracked_to(
            ADA,
            day,
            "gpt-6.1-sol",
            to_unassigned(AiUnassignedKind::Unattributed),
            30 * CREDIT,
        ),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 120),
            line("user 1", "project", "unattributed", 30),
            line("user 1", "untracked", CREDITS_SKU, 50),
        ]
    );
    // The untracked line charges for no tracked usage.
    assert_eq!(allocation.lines[2].usage, AiBillingUsage::default());
}

#[test]
fn a_misrouted_week_bills_at_most_gross_and_flags_settled_days_only() {
    // Grace tracked $40 a day that GitHub billed 5 credits for: six settled
    // days and one within the settle days. Alan's only such day is unsettled.
    let mut input = input(vec![
        linked(1002, "grace-hopper", GRACE),
        linked(1003, "alan-turing", ALAN),
    ]);
    let settling = date!(2026 - 10 - 23);
    let mut days: Vec<Date> = (13..=18)
        .map(|day| Date::from_calendar_date(2026, time::Month::October, day).unwrap())
        .collect();
    days.push(settling);
    for day in &days {
        input
            .account_credits
            .push(credits(1002, *day, "GPT-6.1 Sol", "5"));
        input
            .tracked
            .push(tracked(GRACE, *day, "gpt-6.1-sol", 4000 * CREDIT));
    }
    input
        .account_credits
        .push(credits(1003, settling, "GPT-6.1 Sol", "5"));
    input
        .tracked
        .push(tracked(ALAN, settling, "gpt-6.1-sol", 4000 * CREDIT));
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 3", "project", WIDGET, 5),
            line("user 2", "project", WIDGET, 35),
        ]
    );
    assert_eq!(allocation.low_ratio_groups, 6);
    assert!(!allocation.lines[0].charge.low_ratio);
    assert!(allocation.lines[1].charge.low_ratio);
    // October is in progress, today's slice incomplete, and the last days
    // may still change.
    assert_eq!(
        allocation.lines[1].charge.provisional,
        [
            AiMeteredProvisional::MonthInProgress,
            AiMeteredProvisional::ImportIncomplete,
            AiMeteredProvisional::NotSettled,
        ]
    );
    assert!(allocation.provisional);
}

#[test]
fn an_auto_routed_over_estimate_splits_gross_unflagged() {
    // GitHub's Auto gross already includes its discount: 0.9 of the list.
    let day = date!(2026 - 10 - 07);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "Auto: GPT-6.1 Sol", "90")];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 100 * CREDIT)];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "project", WIDGET, 90)]
    );
    assert!(!allocation.lines[0].charge.low_ratio);
    assert_eq!(allocation.low_ratio_groups, 0);
}

#[test]
fn cloud_agent_credits_bill_as_the_users_untracked_copilot() {
    let day = date!(2026 - 10 - 09);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "50"),
        row(
            1001,
            day,
            AGENT_SKU,
            "GPT-6.1 Sol",
            AiCreditKind::Untrackable,
            "12",
        ),
    ];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 50 * CREDIT)];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 50),
            line("user 1", "untracked", AGENT_SKU, 12),
        ]
    );
}

#[test]
fn unlinked_and_external_accounts_bill_as_account_lines() {
    let mut input = closed_input(vec![
        // Ada is linked from the 10th; before that her account is unlinked.
        account(
            1001,
            "ada-lovelace",
            true,
            &[(AiAccountTarget::User(ADA), date!(2026 - 10 - 10), None)],
        ),
        account(
            1004,
            "kj-contractor",
            true,
            &[(AiAccountTarget::External, date!(2026 - 09 - 01), None)],
        ),
    ]);
    input.account_credits = vec![
        credits(1001, date!(2026 - 10 - 05), "GPT-6.1 Sol", "30"),
        credits(1001, date!(2026 - 10 - 12), "GPT-6.1 Sol", "10"),
        credits(1004, date!(2026 - 10 - 12), "GPT-5.4", "70"),
    ];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 05), "gpt-6.1-sol", 25 * CREDIT),
        tracked(ADA, date!(2026 - 10 - 12), "gpt-6.1-sol", 10 * CREDIT),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 10),
            line("account ada-lovelace", "unlinked", CREDITS_SKU, 30),
            line("account kj-contractor", "external", CREDITS_SKU, 70),
        ]
    );
    assert_eq!(
        allocation.lines[2].subject,
        AiBillingSubject::ProviderAccount {
            provider: AiProvider::Copilot,
            account_id: account_id(1004),
            login: AiProviderLogin::parse("kj-contractor").unwrap(),
        }
    );
    // Ada's usage on her unlinked day is on no line.
    assert_eq!(
        allocation.unbilled.tracked_unlinked.priced_cost,
        usd(25 * CREDIT)
    );
}

#[test]
fn code_review_is_left_out_of_the_account_lines() {
    let day = date!(2026 - 10 - 09);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "50"),
        row(
            1001,
            day,
            CREDITS_SKU,
            "Code Review model",
            AiCreditKind::RepositoryBilled,
            "20",
        ),
    ];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 50 * CREDIT)];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "project", WIDGET, 50)]
    );
    let account = &reconcile(input).unwrap().accounts[0];
    assert_eq!(account.gross, usd(70 * CREDIT));
    assert_eq!(account.repository_billed, usd(20 * CREDIT));
}

#[test]
fn the_unknown_account_bills_the_positive_residual_without_code_review() {
    let (settled, gap, over, settling) = (
        date!(2026 - 10 - 02),
        date!(2026 - 10 - 03),
        date!(2026 - 10 - 04),
        date!(2026 - 10 - 23),
    );
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![
        credits(1001, settled, "GPT-6.1 Sol", "10"),
        credits(1001, gap, "GPT-6.1 Sol", "10"),
        credits(1001, over, "GPT-6.1 Sol", "12"),
        credits(1001, settling, "GPT-6.1 Sol", "10"),
    ];
    input.org_credits = vec![
        // Five credits of an account Toki never listed.
        org(settled, "GPT-6.1 Sol", "15"),
        // Code review is billed per repository: never an unknown account.
        org_row(
            settled,
            CREDITS_SKU,
            "Code Review model",
            AiCreditKind::RepositoryBilled,
            "7",
        ),
        // An import gap: left out.
        org(gap, "GPT-6.1 Sol", "13"),
        // The accounts exceed the organization: it nets.
        org(over, "GPT-6.1 Sol", "10"),
        // Still settling: counted, provisionally.
        org(settling, "GPT-6.1 Sol", "11"),
    ];
    input.coverage.get_mut(&gap).unwrap().accounts_missing = 1;
    let allocation = allocate(&input);
    let unknown = allocation.lines.last().unwrap();
    assert_eq!(
        unknown.subject,
        AiBillingSubject::UnknownAccount {
            provider: AiProvider::Copilot
        }
    );
    assert_eq!(
        unknown.charge.kind,
        AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown)
    );
    assert_eq!(unknown.charge.credits(), 5 + 1 - 2);
    assert_eq!(unknown.charge.account, None);
    assert_eq!(
        unknown.charge.provisional,
        [
            AiMeteredProvisional::MonthInProgress,
            AiMeteredProvisional::ImportIncomplete,
            AiMeteredProvisional::NotSettled,
        ]
    );
    // The reconciliation tab's unknown account includes the code review.
    assert_eq!(
        reconcile(input.clone()).unwrap().totals.unknown_account,
        (5 + 7 + 1 - 2) * CREDIT
    );

    // A negative residual bills no line.
    input.org_credits = vec![org(over, "GPT-6.1 Sol", "10")];
    let allocation = allocate(&input);
    assert!(allocation.lines.iter().all(|line| line.subject
        != AiBillingSubject::UnknownAccount {
            provider: AiProvider::Copilot
        }));
}

#[test]
fn gross_without_weights_is_untracked_and_tracking_without_gross_bills_nothing() {
    let day = date!(2026 - 10 - 08);
    let mut input = closed_input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    // Ada's usage is all unpriced: no weight, so all of G is untracked.
    input.account_credits = vec![credits(1001, day, "Claude Sonnet 5.5", "40")];
    input.tracked = vec![
        AiTrackedModelDay {
            unpriced_records: 12,
            records: 12,
            ..tracked(ADA, day, "claude-sonnet-5.5", 0)
        },
        // GitHub billed Grace nothing that day.
        tracked(GRACE, day, "gpt-6.1-sol", 20 * CREDIT),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 0),
            line("user 1", "untracked", CREDITS_SKU, 40),
            line("user 2", "project", WIDGET, 0),
        ]
    );
    assert_eq!(allocation.lines[0].usage.unpriced_records, 12);
    assert_eq!(allocation.lines[2].usage.priced_cost, usd(20 * CREDIT));
}

#[test]
fn free_models_carry_no_weight_and_bill_nothing() {
    let day = date!(2026 - 10 - 06);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    input.tracked = vec![
        tracked(ADA, day, "copilot-nes-oct", 5 * CREDIT),
        tracked(ADA, day, "gpt-4o-mini-2024-07-18", 3 * CREDIT),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "untracked", CREDITS_SKU, 10)]
    );
    assert_eq!(allocation.unbilled.free_models.priced_cost, usd(8 * CREDIT));
}

#[test]
fn personal_usage_bills_as_a_personal_line() {
    let day = date!(2026 - 10 - 06);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "30")];
    input.tracked = vec![tracked_to(
        ADA,
        day,
        "gpt-6.1-sol",
        AiMeteredDestination::Personal,
        30 * CREDIT,
    )];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "personal", CREDITS_SKU, 30)]
    );
    assert_eq!(allocation.lines[0].project, None);
    assert_eq!(allocation.lines[0].unassigned, None);
}

fn shared_login() -> AiReconciliationInput {
    let account = account(
        1001,
        "shared-login",
        true,
        &[
            (
                AiAccountTarget::User(ADA),
                date!(2026 - 09 - 01),
                Some(date!(2026 - 10 - 14)),
            ),
            (AiAccountTarget::User(GRACE), date!(2026 - 10 - 15), None),
        ],
    );
    let mut input = closed_input(vec![account]);
    input.account_credits = vec![
        credits(1001, date!(2026 - 10 - 10), "GPT-5.4", "40"),
        credits(1001, date!(2026 - 10 - 20), "GPT-5.4", "60"),
    ];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 10), "gpt-5.4", 40 * CREDIT),
        tracked(GRACE, date!(2026 - 10 - 20), "gpt-5.4", 30 * CREDIT),
        // Before Grace's disposition: on no line.
        tracked(GRACE, date!(2026 - 10 - 11), "gpt-5.4", 9 * CREDIT),
    ];
    input
}

#[test]
fn an_owner_change_bills_both_users_under_one_rounding() {
    let allocation = allocate(&shared_login());
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 40),
            line("user 2", "project", WIDGET, 30),
            line("user 2", "untracked", CREDITS_SKU, 30),
        ]
    );
}

#[test]
fn one_users_part_keeps_their_lines_unbilled_usage_and_counters() {
    let mut input = shared_login();
    // Grace's day without a slice.
    input
        .account_slices
        .remove(&(account_id(1001), date!(2026 - 10 - 22)));
    input
        .tracked
        .push(tracked(GRACE, date!(2026 - 10 - 22), "gpt-5.4", 2 * CREDIT));
    let everyone = allocate(&input);
    assert_eq!(everyone.days_not_imported, 1);
    let grace = for_user(everyone.clone(), GRACE);
    assert_eq!(
        summary(&grace),
        [
            line("user 2", "project", WIDGET, 30),
            line("user 2", "untracked", CREDITS_SKU, 30),
        ]
    );
    assert_eq!(grace.unbilled.tracked_unlinked.priced_cost, usd(9 * CREDIT));
    assert_eq!(grace.unbilled.not_imported.priced_cost, usd(2 * CREDIT));
    assert_eq!(grace.days_not_imported, 1);
    assert!(grace.provisional);
    let ada = for_user(everyone, ADA);
    assert_eq!(summary(&ada), [line("user 1", "project", WIDGET, 40)]);
    assert_eq!(ada.unbilled, AiMeteredUnbilled::default());
    assert_eq!(ada.days_not_imported, 0);
    // The shared account's lines are provisional for everyone.
    assert!(ada.provisional);
}

#[test]
fn an_account_that_may_still_change_keeps_its_users_part_provisional_without_a_line() {
    // Ada's account has neither credits nor tracked usage yet; Grace's bills.
    let day = date!(2026 - 10 - 12);
    let mut input = closed_input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    input.account_credits = vec![credits(1002, day, "GPT-6.1 Sol", "10")];
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "10")];
    input.tracked = vec![tracked(GRACE, day, "gpt-6.1-sol", 10 * CREDIT)];
    // (the whole bill, Ada's part, Grace's part) provisional.
    let provisional = |input: &AiReconciliationInput| {
        let allocation = allocate(input);
        assert_eq!(
            summary(&allocation),
            [line("user 2", "project", WIDGET, 10)]
        );
        assert_eq!(for_user(allocation.clone(), ADA).lines, []);
        (
            allocation.provisional,
            for_user(allocation.clone(), ADA).provisional,
            for_user(allocation, GRACE).provisional,
        )
    };
    assert_eq!(provisional(&input), (false, false, false));

    // A day of Ada's account never imported: GitHub may still bill it, say
    // for the cloud agent, which Toki never tracks.
    let mut missing = input.clone();
    let gap = date!(2026 - 10 - 20);
    missing.account_slices.remove(&(account_id(1001), gap));
    missing.coverage.get_mut(&gap).unwrap().accounts_missing = 1;
    assert_eq!(for_user(allocate(&missing), ADA).days_not_imported, 1);
    assert_eq!(provisional(&missing), (true, true, false));

    // Her account's stored month differs from GitHub's month total.
    let mut unconfirmed = input.clone();
    for check in &mut unconfirmed.checks {
        if check.account_id == Some(account_id(1001)) {
            check.payer_gross = Some(dec("3"));
            check.matches = Some(false);
        }
    }
    assert_eq!(provisional(&unconfirmed), (true, true, false));

    // Days before the import's backfill: imported for no account, so
    // Grace's line is provisional too.
    let mut before_backfill = input;
    let backfill_from = date!(2026 - 10 - 10);
    before_backfill
        .coverage
        .retain(|day, _| *day >= backfill_from);
    before_backfill
        .account_slices
        .retain(|(_, day), _| *day >= backfill_from);
    assert_eq!(provisional(&before_backfill), (true, true, true));
}

#[test]
fn rounding_cents_never_charge_a_destination_above_its_tracked_cost() {
    let day = date!(2026 - 10 - 06);
    let mut input = closed_input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "1.5"),
        credits(1002, day, "GPT-6.1 Sol", "1.5"),
    ];
    input.tracked = vec![
        tracked(ADA, day, "gpt-6.1-sol", 15 * CREDIT / 10),
        tracked_to(
            GRACE,
            day,
            "gpt-6.1-sol",
            AiMeteredDestination::Personal,
            15 * CREDIT / 10,
        ),
    ];
    let allocation = allocate(&input);
    // 1.5 credits round to 2: the project and the personal line are capped
    // at their 1.5 tracked, rounded down, and each cent goes to the same
    // user's untracked line.
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 1),
            line("user 1", "untracked", CREDITS_SKU, 1),
            line("user 2", "personal", CREDITS_SKU, 1),
            line("user 2", "untracked", CREDITS_SKU, 1),
        ]
    );
    assert_eq!(allocation.lines[1].charge.allocated, AiUsdNanos::ZERO);
    assert_eq!(allocation.lines[3].charge.allocated, AiUsdNanos::ZERO);
}

#[test]
fn a_personal_line_that_wins_the_rounding_cent_hands_it_to_untracked() {
    let day = date!(2026 - 10 - 06);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    input.tracked = vec![
        tracked(ADA, day, "gpt-6.1-sol", 12 * CREDIT / 10),
        tracked_to(
            ADA,
            day,
            "gpt-6.1-sol",
            AiMeteredDestination::Personal,
            16 * CREDIT / 10,
        ),
    ];
    let allocation = allocate(&input);
    // T = 2.8 < G = 10: 1.2 to the project, 1.6 to personal, 7.2 untracked.
    // Largest remainder gives the spare cent to personal (0.6), which would
    // bill 2 against its 1.6 tracked; the cap moves it to untracked.
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 1),
            line("user 1", "personal", CREDITS_SKU, 1),
            line("user 1", "untracked", CREDITS_SKU, 8),
        ]
    );
    assert_eq!(allocation.lines[1].charge.allocated, usd(16 * CREDIT / 10));
    assert_eq!(allocation.lines[2].charge.allocated, usd(72 * CREDIT / 10));
}

#[test]
fn lines_say_why_they_are_provisional() {
    let day = date!(2026 - 10 - 05);
    let user = UserId::new;
    let logins = [
        (1001, "ada-lovelace"),
        (1002, "grace-hopper"),
        (1003, "alan-turing"),
        (1004, "edsger-dijkstra"),
        (1005, "barbara-liskov"),
        (1006, "donald-knuth"),
        (1007, "margaret-hamilton"),
        (1008, "john-backus"),
        (1010, "ken-thompson"),
        (1011, "frances-allen"),
    ];
    let mut accounts: Vec<AiStoredProviderAccount> = logins
        .iter()
        .map(|(id, login)| linked(*id, login, user((*id - 1000) as i32)))
        .collect();
    // Departed: no seat this month, and its import failed.
    accounts.push(account(
        1009,
        "former-member",
        false,
        &[(AiAccountTarget::User(user(9)), date!(2026 - 09 - 01), None)],
    ));
    let mut closed = closed_input(accounts);
    for id in [1001, 1002, 1003, 1004, 1005, 1006, 1008, 1009, 1010] {
        closed
            .account_credits
            .push(credits(id, day, "GPT-6.1 Sol", "10"));
        closed.tracked.push(tracked(
            user((id - 1000) as i32),
            day,
            "gpt-6.1-sol",
            10 * CREDIT,
        ));
    }
    // An incomplete slice.
    closed.account_slices.insert((account_id(1002), day), false);
    // The account's month total: absent, failed, different.
    closed
        .checks
        .retain(|check| check.account_id != Some(account_id(1003)));
    let failed = |id: i64| AiUsageCheck {
        payer_gross: None,
        matches: None,
        error: Some("the usage source is unavailable".to_string()),
        ..account_check(id)
    };
    for check in &mut closed.checks {
        match check.account_id.map(AiProviderAccountId::as_i64) {
            Some(1004) => *check = failed(1004),
            Some(1005) => check.matches = Some(false),
            Some(1009) => *check = failed(1009),
            _ => {}
        }
    }
    // Usage on an old pricing basis in a group with credits.
    let donald = closed
        .tracked
        .iter_mut()
        .find(|row| row.user_id == user(6))
        .unwrap();
    donald.old_basis_buckets = 2;
    donald.old_basis_cost = usd(10 * CREDIT);
    // A provisional model with tracked usage, and one without.
    closed
        .account_credits
        .push(credits(1007, day, "GPT-6 Astra", "10"));
    closed
        .tracked
        .push(tracked(user(7), day, "gpt-6-astra", 10 * CREDIT));
    closed
        .account_credits
        .push(credits(1011, day, "GPT-6 Astra", "10"));
    // An unrecognized credits row.
    closed.account_credits.push(row(
        1008,
        day,
        "Copilot Mystery",
        "GPT-6.1 Sol",
        AiCreditKind::Unrecognized,
        "5",
    ));
    // Old pricing basis in a group GitHub billed nothing for.
    closed.tracked.push(AiTrackedModelDay {
        old_basis_buckets: 1,
        old_basis_cost: usd(CREDIT),
        ..tracked(user(10), date!(2026 - 10 - 06), "gpt-5.4", CREDIT)
    });
    // The departed account's missing day is no gap.
    closed
        .account_slices
        .remove(&(account_id(1009), date!(2026 - 10 - 20)));

    let allocation = allocate(&closed);
    use AiMeteredProvisional::*;
    for (id, expected) in [
        (1, vec![]),
        (2, vec![ImportIncomplete]),
        (3, vec![NotSettled]),
        (4, vec![NotSettled]),
        (5, vec![NotSettled]),
        (6, vec![OldPricingBasis]),
        (7, vec![ProvisionalModel]),
        (8, vec![Unrecognized]),
        (9, vec![]),
        (10, vec![]),
        (11, vec![]),
    ] {
        assert_eq!(reasons(&allocation, id), expected, "user {id}");
    }
    assert_eq!(allocation.days_not_imported, 0);
    assert!(allocation.provisional);

    // October in progress: today's slice is incomplete, the last days may
    // still change.
    let mut current = input(vec![linked(1001, "ada-lovelace", ADA)]);
    current.checks.push(account_check(1001));
    current.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    current.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT)];
    assert_eq!(
        reasons(&allocate(&current), 1),
        [MonthInProgress, ImportIncomplete, NotSettled]
    );
}

#[test]
fn usage_on_days_before_the_backfill_is_not_imported() {
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    let backfill_from = date!(2026 - 10 - 10);
    input.coverage.retain(|day, _| *day >= backfill_from);
    input
        .account_slices
        .retain(|(_, day), _| *day >= backfill_from);
    input.account_credits = vec![credits(1001, date!(2026 - 10 - 12), "GPT-6.1 Sol", "10")];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 12), "gpt-6.1-sol", 10 * CREDIT),
        tracked(ADA, date!(2026 - 10 - 05), "gpt-6.1-sol", 7 * CREDIT),
    ];
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "project", WIDGET, 10)]
    );
    assert_eq!(
        allocation.unbilled.not_imported.priced_cost,
        usd(7 * CREDIT)
    );
    assert_eq!(
        allocation.lines[0].charge.provisional,
        [AiMeteredProvisional::ImportIncomplete]
    );
    assert!(allocation.provisional);
}

#[test]
fn the_allocation_agrees_with_reconciliation_on_every_account() {
    let day = date!(2026 - 10 - 09);
    let mut input = closed_input(vec![
        linked(1001, "ada-lovelace", ADA),
        account(
            1002,
            "grace-hopper",
            true,
            &[(AiAccountTarget::User(GRACE), date!(2026 - 10 - 12), None)],
        ),
        account(
            1004,
            "kj-contractor",
            true,
            &[(AiAccountTarget::External, date!(2026 - 09 - 01), None)],
        ),
    ]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "100"),
        credits(1001, day, "Claude Sonnet 5.5", "33.3333333"),
        row(
            1001,
            day,
            AGENT_SKU,
            "GPT-6.1 Sol",
            AiCreditKind::Untrackable,
            "12",
        ),
        row(
            1001,
            day,
            CREDITS_SKU,
            "Code Review model",
            AiCreditKind::RepositoryBilled,
            "20",
        ),
        credits(1002, date!(2026 - 10 - 05), "GPT-5.4", "7"),
        credits(1002, date!(2026 - 10 - 14), "GPT-5.4", "70"),
        credits(1004, day, "GPT-5.4", "9.99"),
    ];
    input.tracked = vec![
        tracked_to(ADA, day, "gpt-6.1-sol", to_project(widget()), 70 * CREDIT),
        tracked_to(ADA, day, "gpt-6.1-sol", to_project(portal()), 50 * CREDIT),
        tracked_to(
            ADA,
            day,
            "gpt-6.1-sol",
            AiMeteredDestination::Personal,
            30 * CREDIT,
        ),
        tracked(ADA, day, "claude-sonnet-5.5", 11 * CREDIT),
        tracked(GRACE, date!(2026 - 10 - 14), "gpt-5.4", 100 * CREDIT),
        tracked(GRACE, date!(2026 - 10 - 05), "gpt-5.4", 4 * CREDIT),
    ];
    // `allocate` checks the lines against reconciliation's accounts.
    let allocation = allocate(&input);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 58),
            line("user 1", "project", PORTAL, 33),
            line("user 1", "personal", CREDITS_SKU, 20),
            line("user 1", "untracked", CREDITS_SKU, 22),
            line("user 1", "untracked", AGENT_SKU, 12),
            line("user 2", "project", WIDGET, 70),
            line("account grace-hopper", "unlinked", CREDITS_SKU, 7),
            line("account kj-contractor", "external", CREDITS_SKU, 10),
        ]
    );
    let reconciliation = reconcile(input).unwrap();
    assert_eq!(reconciliation.accounts.len(), 3);
    let billed: i64 = allocation
        .lines
        .iter()
        .map(|line| line.charge.credits())
        .sum();
    let gross =
        reconciliation.totals.gross.nanos() - reconciliation.totals.repository_billed.nanos();
    // Each account rounds once: 145.33, 77 and 9.99 credits bill 145 + 77 + 10.
    assert_eq!((billed, usd(gross).to_cents()), (232, 232));
}

#[test]
fn an_organization_import_gap_keeps_the_month_provisional_without_an_unknown_line() {
    use AiMeteredProvisional::{ImportIncomplete, NotSettled};
    let day = date!(2026 - 10 - 05);
    let mut input = closed_input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    // Five credits of an account Toki never listed.
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "15")];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT)];
    let is_unknown = |line: &AiMeteredLine| {
        line.charge.kind == AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown)
    };
    let settled = allocate(&input);
    assert_eq!(
        summary(&settled),
        [
            line("user 1", "project", WIDGET, 10),
            line("unknown account", "unknown", "", 5),
        ]
    );
    assert!(settled.unknown_account.is_empty());
    assert!(!settled.provisional);

    // The organization's month total failed: every day is an import gap, so
    // the residual is left out, yet the month is not settled at zero.
    let mut failed = input.clone();
    for check in &mut failed.checks {
        if check.account_id.is_none() {
            check.payer_gross = None;
            check.matches = None;
            check.error = Some("the usage source is unavailable".to_string());
        }
    }
    let allocation = allocate(&failed);
    assert_eq!(
        summary(&allocation),
        [line("user 1", "project", WIDGET, 10)]
    );
    assert!(allocation.lines[0].charge.provisional.is_empty());
    assert_eq!(allocation.unknown_account, [ImportIncomplete]);
    assert!(allocation.provisional);
    assert!(allocation.info().provisional);
    // Ada's own lines are settled: the unknown account is no one's.
    let ada = for_user(allocation, ADA);
    assert!(ada.unknown_account.is_empty());
    assert!(!ada.provisional);

    // One organization slice never fetched, on a day without a residual of
    // its own: no line either way, and still provisional.
    let mut missing = input.clone();
    missing.org_credits.clear();
    missing
        .coverage
        .get_mut(&date!(2026 - 10 - 15))
        .unwrap()
        .org_fetched = false;
    let allocation = allocate(&missing);
    assert!(!allocation.lines.iter().any(is_unknown));
    assert_eq!(allocation.unknown_account, [ImportIncomplete]);
    assert!(allocation.provisional);

    // The organization's stored month differs from its month total: the
    // residual bills, provisionally.
    let mut unconfirmed = input;
    for check in &mut unconfirmed.checks {
        if check.account_id.is_none() {
            check.matches = Some(false);
        }
    }
    let allocation = allocate(&unconfirmed);
    let unknown = allocation
        .lines
        .iter()
        .find(|line| is_unknown(line))
        .unwrap();
    assert_eq!(unknown.charge.credits(), 5);
    assert_eq!(unknown.charge.provisional, [NotSettled]);
    assert_eq!(allocation.unknown_account, [NotSettled]);
    assert!(allocation.provisional);
}

#[test]
fn an_interrupted_account_month_stays_provisional_whatever_its_seat() {
    use AiMeteredProvisional::{ImportIncomplete, NotSettled};
    let day = date!(2026 - 10 - 05);
    // Departed: no seat in October, its days still linked to Grace.
    let mut input = closed_input(vec![
        linked(1001, "ada-lovelace", ADA),
        account(
            1009,
            "former-member",
            false,
            &[(AiAccountTarget::User(GRACE), date!(2026 - 09 - 01), None)],
        ),
    ]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "10"),
        credits(1009, day, "GPT-6.1 Sol", "4"),
    ];
    // Five credits of an account Toki never listed.
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "19")];
    input.tracked = vec![
        tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT),
        tracked(GRACE, day, "gpt-6.1-sol", 4 * CREDIT),
    ];
    // A day of the departed account's that was never imported.
    input
        .account_slices
        .remove(&(account_id(1009), date!(2026 - 10 - 20)));
    let set_check = |input: &mut AiReconciliationInput, check: AiUsageCheck| {
        for stored in &mut input.checks {
            if stored.account_id == check.account_id {
                *stored = check.clone();
            }
        }
    };

    // Its own import failed: informational without a seat, so the missing
    // day is no gap and the residual bills.
    let mut own_error = input.clone();
    set_check(
        &mut own_error,
        AiUsageCheck {
            payer_gross: None,
            matches: None,
            error: Some("GitHub knows no Copilot usage for former-member".to_string()),
            ..account_check(1009)
        },
    );
    let allocation = allocate(&own_error);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 10),
            line("user 2", "project", WIDGET, 4),
            line("unknown account", "unknown", "", 5),
        ]
    );
    assert!(reasons(&allocation, 2).is_empty());
    assert_eq!(allocation.days_not_imported, 0);
    assert!(!allocation.provisional);

    // A run stopped before finishing its month: no answer of its own, so its
    // days may be half replaced, its missing day is a gap and the residual
    // is left out, as on the reconciliation tab.
    let mut interrupted = input;
    set_check(
        &mut interrupted,
        AiUsageCheck::interrupted(
            Some(account_id(1009)),
            datetime!(2026-11-02 02:00 UTC),
            dec("4"),
            "GitHub's rate limiting stopped the import",
        ),
    );
    let allocation = allocate(&interrupted);
    assert_eq!(
        summary(&allocation),
        [
            line("user 1", "project", WIDGET, 10),
            line("user 2", "project", WIDGET, 4),
        ]
    );
    assert!(reasons(&allocation, 1).is_empty());
    assert_eq!(reasons(&allocation, 2), [ImportIncomplete, NotSettled]);
    assert_eq!(allocation.days_not_imported, 1);
    assert_eq!(allocation.unknown_account, [ImportIncomplete]);
    assert!(allocation.provisional);
    let reconciliation = reconcile(interrupted).unwrap();
    assert!(reconciliation
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    // Ada's own part stays settled: the unknown account is no one's.
    assert!(!for_user(allocation, ADA).provisional);
}

#[test]
fn only_every_account_can_be_allocated() {
    let input = AiReconciliationInput {
        focus: Some(ADA),
        ..closed_input(vec![linked(1001, "ada-lovelace", ADA)])
    };
    assert!(matches!(
        allocate_metered_credits(&input),
        Err(AiMeteredUsageError::Invalid(_))
    ));
}
