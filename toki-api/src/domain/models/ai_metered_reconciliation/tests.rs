use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{
    AiAccountDisposition, AiAccountDispositionId, AiAccountDispositionTerms, AiAccountPeriod,
    AiMeteredAmounts, AiMeteredModel, AiProviderLogin, AiStoredSeat,
};

const TODAY: Date = date!(2026 - 10 - 25);
const ADA: UserId = UserId::new(1);
const GRACE: UserId = UserId::new(2);
const ALAN: UserId = UserId::new(3);
const CREDITS_SKU: &str = "Copilot AI Credits";
const AGENT_SKU: &str = "Copilot Cloud Agent";
/// Nanodollars per credit.
const CREDIT: i64 = 10_000_000;

fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
}

fn usd(nanos: i64) -> AiUsdNanos {
    AiUsdNanos::new(nanos).unwrap()
}

fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap()
}

fn account_id(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn seat_since(day: Date) -> AiStoredSeat {
    let created_at = day.midnight().assume_utc();
    AiStoredSeat {
        created_at,
        pending_cancellation_on: None,
        last_activity_at: None,
        last_activity_client: None,
        first_listed_at: created_at,
        last_listed_at: datetime!(2026-10-25 06:00 UTC),
        unlisted_at: None,
    }
}

/// An account with a seat since September and the given dispositions.
fn account(
    id: i64,
    login: &str,
    seated: bool,
    dispositions: &[(AiAccountTarget, Date, Option<Date>)],
) -> AiStoredProviderAccount {
    let login = AiProviderLogin::parse(login).unwrap();
    AiStoredProviderAccount {
        account_id: account_id(id),
        profile: None,
        seats: if seated {
            vec![seat_since(date!(2026 - 09 - 01))]
        } else {
            Vec::new()
        },
        dispositions: dispositions
            .iter()
            .enumerate()
            .map(|(index, (target, from, to))| AiAccountDisposition {
                id: AiAccountDispositionId::new(i32::try_from(id).unwrap() * 10 + index as i32),
                payer: AiProvider::Copilot,
                account_id: account_id(id),
                login: login.clone(),
                terms: AiAccountDispositionTerms {
                    target: *target,
                    period: AiAccountPeriod::new(*from, *to).unwrap(),
                },
                updated_at: datetime!(2026-10-01 08:00 UTC),
                updated_by: None,
            })
            .collect(),
        login,
    }
}

fn linked(id: i64, login: &str, user: UserId) -> AiStoredProviderAccount {
    account(
        id,
        login,
        true,
        &[(AiAccountTarget::User(user), date!(2026 - 09 - 01), None)],
    )
}

fn metered_credit(sku: &str, label: &str, kind: AiCreditKind, quantity: &str) -> AiMeteredCredit {
    let label_text = label.to_string();
    let (auto_routed, rest) = match label.strip_prefix("Auto: ") {
        Some(rest) => (true, rest),
        None => (false, label),
    };
    let model = match rest {
        "Code Review model" => "code-review".to_string(),
        rest => rest.to_lowercase().replace(' ', "-"),
    };
    AiMeteredCredit {
        product: "Copilot".to_string(),
        sku: sku.to_string(),
        unit: "ai-credits".to_string(),
        model: AiMeteredModel {
            label: label_text,
            model,
            auto_routed,
        },
        kind,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec(quantity),
            gross_amount: dec(quantity),
            net_quantity: dec(quantity),
            net_amount: dec(quantity),
            ..AiMeteredAmounts::default()
        },
    }
}

fn row(
    id: i64,
    day: Date,
    sku: &str,
    label: &str,
    kind: AiCreditKind,
    quantity: &str,
) -> AiStoredAccountCredit {
    let credit = metered_credit(sku, label, kind, quantity);
    AiStoredAccountCredit {
        account_id: account_id(id),
        day,
        login: format!("account-{id}"),
        gross_usd: credit.gross_usd().unwrap(),
        credit,
        fetched_at: datetime!(2026-10-25 06:00 UTC),
    }
}

/// A trackable credits row.
fn credits(id: i64, day: Date, label: &str, quantity: &str) -> AiStoredAccountCredit {
    row(
        id,
        day,
        CREDITS_SKU,
        label,
        AiCreditKind::Trackable,
        quantity,
    )
}

fn org(day: Date, label: &str, quantity: &str) -> AiStoredOrgCredit {
    let credit = metered_credit(CREDITS_SKU, label, AiCreditKind::Trackable, quantity);
    AiStoredOrgCredit {
        day,
        gross_usd: credit.gross_usd().unwrap(),
        credit,
    }
}

fn tracked(user: UserId, day: Date, model: &str, nanos: i64) -> AiTrackedModelDay {
    AiTrackedModelDay {
        user_id: user,
        day,
        model: model.to_string(),
        priced_cost: usd(nanos),
        records: 10,
        unpriced_records: 0,
        old_basis_cost: AiUsdNanos::ZERO,
        old_basis_buckets: 0,
        before_floor_cost: AiUsdNanos::ZERO,
    }
}

/// Every day of October through today, complete and fetched.
fn full_coverage() -> BTreeMap<Date, AiDayCoverage> {
    let mut coverage = BTreeMap::new();
    let mut day = october().first_day();
    while day <= TODAY {
        coverage.insert(
            day,
            AiDayCoverage {
                org_fetched: true,
                org_complete: day < TODAY,
                accounts_missing: 0,
                accounts_incomplete: u32::from(day == TODAY),
            },
        );
        day = day.next_day().unwrap();
    }
    coverage
}

/// Every account's slice of every covered day, complete before today.
fn all_slices(
    accounts: &[AiStoredProviderAccount],
    coverage: &BTreeMap<Date, AiDayCoverage>,
) -> HashMap<(AiProviderAccountId, Date), bool> {
    accounts
        .iter()
        .flat_map(|account| {
            coverage
                .keys()
                .map(move |day| ((account.account_id, *day), *day < TODAY))
        })
        .collect()
}

/// The organization's check, matching the payer's month total.
fn org_check() -> AiUsageCheck {
    AiUsageCheck {
        account_id: None,
        checked_at: datetime!(2026-10-25 06:00 UTC),
        payer_gross: Some(dec("35")),
        stored_gross: dec("35"),
        matches: Some(true),
        error: None,
        interrupted: false,
    }
}

fn input(accounts: Vec<AiStoredProviderAccount>) -> AiReconciliationInput {
    let coverage = full_coverage();
    AiReconciliationInput {
        account_slices: all_slices(&accounts, &coverage),
        payer: AiProvider::Copilot,
        month: october(),
        today: TODAY,
        settle_days: 3,
        accounts,
        account_credits: Vec::new(),
        org_credits: Vec::new(),
        org_charges: Vec::new(),
        summary: None,
        tracked: Vec::new(),
        checks: vec![org_check()],
        coverage,
        focus: None,
    }
}

/// Reconciles and asserts the identity on every account.
fn run(input: AiReconciliationInput) -> AiMeteredReconciliation {
    let result = reconcile(input).unwrap();
    for account in &result.accounts {
        let parts = account.repository_billed.nanos()
            + account.attributed.nanos()
            + account.untracked.nanos()
            + account.unlinked.nanos()
            + account.external.nanos();
        assert_eq!(account.gross.nanos(), parts, "{}", account.login);
    }
    let totals = result.totals;
    assert_eq!(
        totals.gross.nanos(),
        totals.repository_billed.nanos()
            + totals.attributed.nanos()
            + totals.untracked.nanos()
            + totals.unlinked.nanos()
            + totals.external.nanos()
    );
    result
}

fn account_of<'a>(result: &'a AiMeteredReconciliation, login: &str) -> &'a AiAccountReconciliation {
    result
        .accounts
        .iter()
        .find(|account| account.login == login)
        .unwrap_or_else(|| panic!("{login} is not listed"))
}

fn group<'a>(account: &'a AiAccountReconciliation, model: &str) -> &'a AiReconciliationGroup {
    account
        .groups
        .iter()
        .find(|group| group.model == model)
        .unwrap_or_else(|| panic!("no group {model}"))
}

#[test]
fn attributed_is_the_smaller_of_gross_and_tracked_per_day_and_model() {
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![
        credits(1001, date!(2026 - 10 - 05), "GPT-6.1 Sol", "100"),
        credits(1001, date!(2026 - 10 - 06), "GPT-6.1 Sol", "100"),
        // Auto counts as the model it routed to.
        credits(1001, date!(2026 - 10 - 06), "Auto: GPT-6.1 Sol", "10"),
    ];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 05), "gpt-6.1-sol", 110 * CREDIT),
        tracked(ADA, date!(2026 - 10 - 06), "gpt-6.1-sol", 60 * CREDIT),
    ];
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.gross, usd(210 * CREDIT));
    assert_eq!(ada.tracked, usd(170 * CREDIT));
    // T ≥ G on the 5th: nothing untracked; T < G on the 6th: G − T.
    assert_eq!(ada.attributed, usd(160 * CREDIT));
    assert_eq!(ada.untracked, usd(50 * CREDIT));
    assert_eq!(ada.tracked_share, Some((160 * CREDIT, 210 * CREDIT)));
    let sol = group(ada, "gpt-6.1-sol");
    assert_eq!(sol.labels, ["Auto: GPT-6.1 Sol", "GPT-6.1 Sol"]);
    assert_eq!(sol.auto_gross, usd(10 * CREDIT));
    assert_eq!(sol.status, Some(AiMeteredModelStatus::Confirmed));
    assert_eq!(ada.owners.len(), 1);
    assert_eq!(ada.owners[0].1.start(), date!(2026 - 10 - 01));
    assert_eq!(ada.owners[0].1.last_day(), TODAY);
    // G/T is 100/110 and 110/60: no flags.
    assert!(ada.flags.is_empty(), "{:?}", ada.flags);
}

#[test]
fn material_tracked_usage_without_gross_is_flagged() {
    let day = date!(2026 - 10 - 07);
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
        linked(1003, "alan-turing", ALAN),
    ]);
    input.tracked = vec![
        tracked(ADA, day, "gpt-6.1-sol", 100 * CREDIT),
        tracked(GRACE, day, "gpt-6.1-sol", 100 * CREDIT - CREDIT / 100),
        // Free models carry no weight and are never billed, whatever Toki
        // priced them at and on whatever basis.
        AiTrackedModelDay {
            old_basis_cost: usd(500 * CREDIT),
            old_basis_buckets: 4,
            before_floor_cost: usd(100 * CREDIT),
            unpriced_records: 2,
            ..tracked(ALAN, day, "gpt-4o-mini-2024-07-18", 500 * CREDIT)
        },
    ];
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.flags, [AiReconciliationFlag::TrackedNotBilled]);
    assert_eq!(ada.attributed, AiUsdNanos::ZERO);
    let only_toki = group(ada, "gpt-6.1-sol");
    assert_eq!(only_toki.kind, AiGroupKind::TrackedOnly);
    assert_eq!(only_toki.sku, None);
    assert_eq!(only_toki.flags, [AiReconciliationFlag::TrackedNotBilled]);
    assert!(account_of(&result, "grace-hopper").flags.is_empty());
    let alan = account_of(&result, "alan-turing");
    assert!(alan.flags.is_empty(), "{:?}", alan.flags);
    assert_eq!(alan.tracked, AiUsdNanos::ZERO);
    assert_eq!(alan.tracked_old_basis, AiUsdNanos::ZERO);
    assert_eq!(alan.tracked_before_floor, AiUsdNanos::ZERO);
    assert_eq!(alan.unpriced_records, 0);
    // Listed, with no weight.
    let free = group(alan, "gpt-4o-mini-2024-07-18");
    assert_eq!(free.status, Some(AiMeteredModelStatus::Free));
    assert_eq!(free.kind, AiGroupKind::TrackedOnly);
    assert_eq!(free.tracked, AiUsdNanos::ZERO);
    assert!(free.flags.is_empty(), "{:?}", free.flags);
    assert_eq!(result.totals.tracked, usd(200 * CREDIT - CREDIT / 100));
    assert_eq!(result.totals.tracked_old_basis, AiUsdNanos::ZERO);
    // Flagged accounts come first.
    assert_eq!(result.accounts[0].login, "ada-lovelace");
}

#[test]
fn only_settled_days_raise_the_flags_that_compare_gross_with_tracked() {
    // Today is still being posted; the 23rd is within the three settle days.
    let (settling, settled) = (date!(2026 - 10 - 23), date!(2026 - 10 - 21));
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    input.account_credits = vec![
        credits(1002, TODAY, "GPT-6.1 Sol", "100"),
        credits(1002, settling, "GPT-6.1 Sol", "10"),
    ];
    input.tracked = vec![
        tracked(ADA, TODAY, "gpt-6.1-sol", 300 * CREDIT),
        tracked(ADA, settling, "gpt-6.1-sol", 300 * CREDIT),
        tracked(GRACE, TODAY, "gpt-6.1-sol", 300 * CREDIT),
        tracked(GRACE, settling, "gpt-6.1-sol", 300 * CREDIT),
    ];
    let result = run(input.clone());
    let ada = account_of(&result, "ada-lovelace");
    assert!(ada.flags.is_empty(), "{:?}", ada.flags);
    assert!(group(ada, "gpt-6.1-sol").flags.is_empty());
    // Still counted, only not flagged.
    assert_eq!(ada.tracked, usd(600 * CREDIT));
    let grace = account_of(&result, "grace-hopper");
    assert!(grace.flags.is_empty(), "{:?}", grace.flags);
    assert_eq!(grace.attributed, usd(110 * CREDIT));

    // The same usage on a settled day is flagged.
    input
        .account_credits
        .push(credits(1002, settled, "GPT-6.1 Sol", "10"));
    input.tracked.extend([
        tracked(ADA, settled, "gpt-6.1-sol", 300 * CREDIT),
        tracked(GRACE, settled, "gpt-6.1-sol", 300 * CREDIT),
    ]);
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.flags, [AiReconciliationFlag::TrackedNotBilled]);
    assert_eq!(
        group(ada, "gpt-6.1-sol").flags,
        [AiReconciliationFlag::TrackedNotBilled]
    );
    assert_eq!(
        account_of(&result, "grace-hopper").flags,
        [AiReconciliationFlag::LowRatio]
    );
}

#[test]
fn a_day_without_an_imported_slice_is_not_read_as_billed_nothing() {
    let (imported, missing) = (date!(2026 - 10 - 13), date!(2026 - 10 - 14));
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_slices.remove(&(account_id(1001), missing));
    input.account_credits = vec![credits(1001, imported, "GPT-6.1 Sol", "50")];
    input.tracked = vec![
        tracked(ADA, imported, "gpt-6.1-sol", 50 * CREDIT),
        tracked(ADA, missing, "gpt-6.1-sol", 500 * CREDIT),
    ];
    let result = run(input.clone());
    let ada = account_of(&result, "ada-lovelace");
    assert!(ada.flags.is_empty(), "{:?}", ada.flags);
    assert_eq!(ada.days_not_imported, 1);
    assert_eq!(ada.tracked, usd(50 * CREDIT));
    assert_eq!(ada.attributed, usd(50 * CREDIT));
    // The day is linked, only not compared yet.
    assert!(result.unlinked_tracked.is_empty());

    let focused = run(AiReconciliationInput {
        focus: Some(ADA),
        ..input
    });
    assert_eq!(account_of(&focused, "ada-lovelace").days_not_imported, 1);
    assert!(focused.unlinked_tracked.is_empty());
}

#[test]
fn unpriced_usage_carries_no_weight() {
    let day = date!(2026 - 10 - 08);
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "Claude Sonnet 5.5", "40")];
    input.tracked = vec![AiTrackedModelDay {
        unpriced_records: 12,
        records: 12,
        ..tracked(ADA, day, "claude-sonnet-5.5", 0)
    }];
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.untracked, usd(40 * CREDIT));
    assert_eq!(ada.attributed, AiUsdNanos::ZERO);
    assert_eq!(ada.unpriced_records, 12);
    assert_eq!(ada.tracked_share, Some((0, 40 * CREDIT)));
}

#[test]
fn cloud_agent_credits_are_untracked_and_left_out_of_the_share() {
    let day = date!(2026 - 10 - 09);
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "50"),
        row(
            1001,
            day,
            AGENT_SKU,
            "GPT-6.1 Sol",
            AiCreditKind::Untrackable,
            "30",
        ),
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
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.gross, usd(100 * CREDIT));
    assert_eq!(ada.untrackable, usd(30 * CREDIT));
    assert_eq!(ada.repository_billed, usd(20 * CREDIT));
    assert_eq!(ada.attributed, usd(50 * CREDIT));
    assert_eq!(ada.untracked, usd(30 * CREDIT));
    // Only trackable credits count: 50 of 50.
    assert_eq!(ada.tracked_share, Some((50 * CREDIT, 50 * CREDIT)));
    let agent = ada
        .groups
        .iter()
        .find(|group| group.kind == AiGroupKind::Credits(AiCreditKind::Untrackable))
        .unwrap();
    assert_eq!(agent.tracked, AiUsdNanos::ZERO);
    assert_eq!(agent.untracked, usd(30 * CREDIT));
    let review = group(ada, "code-review");
    assert_eq!(review.status, None);
    assert!(review.flags.is_empty());
    assert_eq!(review.attributed, AiUsdNanos::ZERO);
}

#[test]
fn unlinked_and_external_days_keep_their_credits_apart() {
    let mut input = input(vec![
        // Ada is linked from the 10th; before that the account is unlinked.
        account(
            1001,
            "ada-lovelace",
            true,
            &[(AiAccountTarget::User(ADA), date!(2026 - 10 - 10), None)],
        ),
        account(
            1004,
            "octo-bot",
            true,
            &[(AiAccountTarget::External, date!(2026 - 09 - 01), None)],
        ),
    ]);
    input.account_credits = vec![
        credits(1001, date!(2026 - 10 - 05), "GPT-6.1 Sol", "30"),
        credits(1001, date!(2026 - 10 - 06), "GPT-6.1 Sol", "5"),
        credits(1001, date!(2026 - 10 - 12), "GPT-6.1 Sol", "10"),
        credits(1004, date!(2026 - 10 - 12), "GPT-5.4", "70"),
    ];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 05), "gpt-6.1-sol", 25 * CREDIT),
        tracked(ADA, date!(2026 - 10 - 12), "gpt-6.1-sol", 10 * CREDIT),
        // Free models carry no weight on unlinked days either.
        tracked(ADA, date!(2026 - 10 - 05), "copilot-nes-oct", 5 * CREDIT),
        tracked(ADA, date!(2026 - 10 - 06), "copilot-nes-oct", 5 * CREDIT),
    ];
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(ada.unlinked, usd(35 * CREDIT));
    assert_eq!(ada.unlinked_days, 2);
    assert_eq!(ada.attributed, usd(10 * CREDIT));
    assert_eq!(ada.tracked, usd(10 * CREDIT));
    assert!(ada.flags.contains(&AiReconciliationFlag::Unlinked));
    // Ada's tracked usage on the unlinked 5th is unlinked tracked usage; the
    // 6th has only a free model's.
    assert_eq!(
        result.unlinked_tracked,
        [AiUnlinkedTracked {
            user_id: ADA,
            tracked: usd(25 * CREDIT),
            days: 1,
        }]
    );
    let bot = account_of(&result, "octo-bot");
    assert_eq!(bot.external, usd(70 * CREDIT));
    assert_eq!(bot.flags, [AiReconciliationFlag::External]);
    assert_eq!(result.totals.unlinked, usd(35 * CREDIT));
    assert_eq!(result.totals.external, usd(70 * CREDIT));
}

#[test]
fn an_owner_change_splits_the_month_and_focus_shows_one_user() {
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
    let mut input = input(vec![account]);
    input.account_credits = vec![
        credits(1001, date!(2026 - 10 - 10), "GPT-5.4", "40"),
        credits(1001, date!(2026 - 10 - 20), "GPT-5.4", "60"),
    ];
    input.tracked = vec![
        tracked(ADA, date!(2026 - 10 - 10), "gpt-5.4", 40 * CREDIT),
        tracked(GRACE, date!(2026 - 10 - 20), "gpt-5.4", 30 * CREDIT),
    ];
    let everyone = run(input.clone());
    let shared = account_of(&everyone, "shared-login");
    assert_eq!(shared.owners.len(), 2);
    assert_eq!(shared.attributed, usd(70 * CREDIT));
    assert_eq!(shared.untracked, usd(30 * CREDIT));

    let grace = run(AiReconciliationInput {
        focus: Some(GRACE),
        ..input
    });
    let shared = account_of(&grace, "shared-login");
    assert_eq!(shared.gross, usd(60 * CREDIT));
    assert_eq!(shared.attributed, usd(30 * CREDIT));
    assert_eq!(shared.owners.len(), 1);
    assert_eq!(shared.owners[0].0, AiAccountTarget::User(GRACE));
    assert!(grace.residual_days.is_empty());
    assert!(grace.repositories.is_empty());
    assert!(grace.pool.is_none());
}

#[test]
fn groups_billed_below_85_percent_of_tracked_are_flagged() {
    let day = date!(2026 - 10 - 11);
    let flags = |gross: &str, tracked_nanos: i64, label: &str| {
        let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
        input.account_credits = vec![credits(1001, day, label, gross)];
        input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", tracked_nanos)];
        let result = run(input);
        let ada = account_of(&result, "ada-lovelace");
        (ada.flags.clone(), group(ada, "gpt-6.1-sol").flags.clone())
    };
    let low = vec![AiReconciliationFlag::LowRatio];
    assert_eq!(flags("85", 100 * CREDIT, "GPT-6.1 Sol"), (vec![], vec![]));
    assert_eq!(
        flags("84.99", 100 * CREDIT, "GPT-6.1 Sol"),
        (low.clone(), low.clone())
    );
    // However little was tracked: 20 credits billed of 60 tracked.
    assert_eq!(
        flags("20", 60 * CREDIT, "GPT-6.1 Sol"),
        (low.clone(), low.clone())
    );
    assert_eq!(
        flags("0.5", CREDIT, "GPT-6.1 Sol"),
        (low.clone(), low.clone())
    );
    // Used only through Auto: exactly 0.9 of the list price.
    assert_eq!(
        flags("180", 200 * CREDIT, "Auto: GPT-6.1 Sol"),
        (vec![], vec![])
    );
}

#[test]
fn stale_pricing_unknown_models_and_check_problems_are_flagged() {
    let day = date!(2026 - 10 - 12);
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
        linked(1003, "alan-turing", ALAN),
        // Departed: no seat, no disposition.
        account(1005, "former-member", false, &[]),
    ]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "10"),
        // Another SKU and a wrong unit are unrecognized rows.
        row(
            1002,
            day,
            "Copilot Mystery",
            "GPT-6.1 Sol",
            AiCreditKind::Unrecognized,
            "5",
        ),
        credits(1003, day, "Brand New Model", "5"),
    ];
    input.tracked = vec![
        AiTrackedModelDay {
            old_basis_cost: usd(10 * CREDIT),
            old_basis_buckets: 3,
            before_floor_cost: AiUsdNanos::ZERO,
            ..tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT)
        },
        // A Toki model outside the catalog with tracked cost.
        tracked(GRACE, day, "mystery-model", CREDIT),
    ];
    input.checks.extend([
        AiUsageCheck {
            account_id: Some(account_id(1001)),
            checked_at: datetime!(2026-10-25 06:00 UTC),
            payer_gross: Some(dec("10.5")),
            stored_gross: dec("10"),
            matches: Some(false),
            error: None,
            interrupted: false,
        },
        AiUsageCheck {
            account_id: Some(account_id(1003)),
            checked_at: datetime!(2026-10-25 06:00 UTC),
            payer_gross: None,
            stored_gross: dec("5"),
            matches: None,
            error: Some("unexpected usage source response".to_string()),
            interrupted: false,
        },
        AiUsageCheck {
            account_id: Some(account_id(1005)),
            checked_at: datetime!(2026-10-25 06:00 UTC),
            payer_gross: None,
            stored_gross: AiDecimal::ZERO,
            matches: None,
            error: Some("GitHub knows no Copilot usage for former-member".to_string()),
            interrupted: false,
        },
    ]);
    let result = run(input);
    let ada = account_of(&result, "ada-lovelace");
    assert_eq!(
        ada.flags,
        [
            AiReconciliationFlag::StalePricing,
            AiReconciliationFlag::PayerTotalMismatch
        ]
    );
    assert_eq!(ada.tracked_old_basis, usd(10 * CREDIT));
    let grace = account_of(&result, "grace-hopper");
    assert!(grace.flags.contains(&AiReconciliationFlag::UnknownModel));
    assert_eq!(grace.attributed, AiUsdNanos::ZERO);
    assert_eq!(grace.untracked, usd(5 * CREDIT));
    let alan = account_of(&result, "alan-turing");
    assert_eq!(
        alan.flags,
        [
            AiReconciliationFlag::UnknownModel,
            AiReconciliationFlag::ImportFailed
        ]
    );
    // The departed account's error is listed but not an import failure.
    let former = account_of(&result, "former-member");
    assert!(former.flags.is_empty(), "{:?}", former.flags);
    assert!(former.check.as_ref().unwrap().error.is_some());
}

#[test]
fn the_residual_is_the_org_total_minus_every_account() {
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    let (settled, settling, gap, over, rounding) = (
        date!(2026 - 10 - 02),
        date!(2026 - 10 - 23),
        date!(2026 - 10 - 03),
        date!(2026 - 10 - 04),
        date!(2026 - 10 - 05),
    );
    input.account_credits = vec![
        credits(1001, settled, "GPT-6.1 Sol", "10"),
        credits(1002, settled, "GPT-5.4", "20"),
        credits(1001, settling, "GPT-6.1 Sol", "10"),
        credits(1001, gap, "GPT-6.1 Sol", "10"),
        credits(1001, over, "GPT-6.1 Sol", "12"),
        credits(1001, rounding, "GPT-6.1 Sol", "1.000005"),
    ];
    input.org_credits = vec![
        // Five credits more than the accounts: an account Toki never listed.
        org(settled, "GPT-6.1 Sol", "15"),
        org(settled, "GPT-5.4", "20"),
        org(settling, "GPT-6.1 Sol", "11"),
        org(gap, "GPT-6.1 Sol", "13"),
        org(over, "GPT-6.1 Sol", "10"),
        org(rounding, "GPT-6.1 Sol", "1"),
    ];
    input.coverage.get_mut(&gap).unwrap().accounts_missing = 1;
    let result = run(input);
    let day = |day: Date| {
        result
            .residual_days
            .iter()
            .find(|residual| residual.day == day)
            .unwrap()
    };
    let settled_day = day(settled);
    assert_eq!(settled_day.status, AiResidualStatus::Settled);
    assert_eq!(settled_day.org_gross, usd(35 * CREDIT));
    assert_eq!(settled_day.accounts_gross, usd(30 * CREDIT));
    assert_eq!(settled_day.residual, 5 * CREDIT);
    assert_eq!(
        settled_day.groups,
        [AiResidualGroup {
            sku: CREDITS_SKU.to_string(),
            model: "gpt-6.1-sol".to_string(),
            residual: 5 * CREDIT,
        }]
    );
    assert_eq!(day(settling).status, AiResidualStatus::Provisional);
    assert_eq!(day(gap).status, AiResidualStatus::Incomplete);
    assert!(day(over).accounts_exceed_org);
    assert_eq!(day(over).residual, -2 * CREDIT);
    // 50 nanodollars is rounding, not a residual.
    assert_eq!(day(rounding).residual, 0);
    assert!(day(rounding).groups.is_empty());
    assert_eq!(result.totals.unknown_account, (5 + 1 - 2) * CREDIT);
    assert_eq!(result.totals.unknown_account_settled, (5 - 2) * CREDIT);
    assert_eq!(result.totals.incomplete_import, 3 * CREDIT);
    // Every day of the month through today is reconciled.
    assert_eq!(result.residual_days.len(), 25);
}

#[test]
fn no_day_settles_until_the_org_total_equals_the_payers() {
    let (settled, settling) = (date!(2026 - 10 - 02), date!(2026 - 10 - 23));
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, settled, "GPT-6.1 Sol", "10")];
    input.org_credits = vec![
        org(settled, "GPT-6.1 Sol", "15"),
        org(settling, "GPT-6.1 Sol", "1"),
    ];
    let statuses = |result: &AiMeteredReconciliation| {
        let status = |day: Date| {
            result
                .residual_days
                .iter()
                .find(|residual| residual.day == day)
                .unwrap()
                .status
        };
        (status(settled), status(settling))
    };

    // Confirmed: the settled day's five credits are unknown account.
    let confirmed = run(input.clone());
    assert_eq!(
        statuses(&confirmed),
        (AiResidualStatus::Settled, AiResidualStatus::Provisional)
    );
    assert_eq!(confirmed.totals.unknown_account_settled, 5 * CREDIT);
    assert_eq!(confirmed.org_check, Some(org_check()));

    // A usage post landed between the month total and the day calls: the
    // stored days differ from the payer's month total, so nothing settles.
    let mismatch = AiUsageCheck {
        payer_gross: Some(dec("14")),
        stored_gross: dec("16"),
        matches: Some(false),
        ..org_check()
    };
    input.checks = vec![mismatch.clone()];
    let mismatched = run(input.clone());
    assert_eq!(
        statuses(&mismatched),
        (AiResidualStatus::Provisional, AiResidualStatus::Provisional)
    );
    assert_eq!(mismatched.totals.unknown_account, 6 * CREDIT);
    assert_eq!(mismatched.totals.unknown_account_settled, 0);
    assert_eq!(mismatched.org_check, Some(mismatch));

    // Never checked: not confirmed either.
    input.checks = Vec::new();
    let unchecked = run(input.clone());
    assert_eq!(
        statuses(&unchecked),
        (AiResidualStatus::Provisional, AiResidualStatus::Provisional)
    );
    assert_eq!(unchecked.org_check, None);

    // A failed check: every day is an import gap, not unknown account.
    input.checks = vec![AiUsageCheck {
        payer_gross: None,
        matches: None,
        error: Some("the usage source is unavailable".to_string()),
        ..org_check()
    }];
    let failed = run(input.clone());
    assert!(failed
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(failed.totals.unknown_account, 0);
    assert_eq!(failed.totals.incomplete_import, 6 * CREDIT);
    // The organization's check never flags an account.
    assert!(account_of(&failed, "ada-lovelace").flags.is_empty());

    // One user's view has no residual and no organization check.
    let focused = run(AiReconciliationInput {
        focus: Some(ADA),
        ..input
    });
    assert_eq!(focused.org_check, None);
}

#[test]
fn an_interrupted_account_counts_whatever_its_seat() {
    // A departed account, unseated in October, whose month a run stopped
    // importing: not its own error, so it is flagged and every day is an
    // import gap rather than unknown account.
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        account(1005, "former-member", false, &[]),
    ]);
    input.org_credits = vec![org(date!(2026 - 10 - 02), "GPT-6.1 Sol", "15")];
    input.checks.push(AiUsageCheck::interrupted(
        Some(account_id(1005)),
        datetime!(2026-10-25 06:00 UTC),
        AiDecimal::ZERO,
        "GitHub's rate limiting stopped the import",
    ));
    let result = run(input.clone());
    let former = account_of(&result, "former-member");
    assert_eq!(former.flags, [AiReconciliationFlag::ImportFailed]);
    let check = former.check.as_ref().unwrap();
    assert!(check.interrupted);
    assert_eq!(
        check.error.as_deref(),
        Some("the run stopped before finishing this month: GitHub's rate limiting stopped the import")
    );
    assert!(result
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(result.totals.unknown_account, 0);

    // Its own error would have been informational.
    input.checks.pop();
    input.checks.push(AiUsageCheck {
        account_id: Some(account_id(1005)),
        checked_at: datetime!(2026-10-25 06:00 UTC),
        payer_gross: None,
        stored_gross: AiDecimal::ZERO,
        matches: None,
        error: Some("GitHub knows no Copilot usage for former-member".to_string()),
        interrupted: false,
    });
    let result = run(input);
    assert!(account_of(&result, "former-member").flags.is_empty());
    assert_eq!(result.totals.unknown_account, 15 * CREDIT);
}

#[test]
fn a_seated_account_that_failed_makes_every_day_an_import_gap() {
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.org_credits = vec![org(date!(2026 - 10 - 02), "GPT-6.1 Sol", "15")];
    input.checks.push(AiUsageCheck {
        account_id: Some(account_id(1001)),
        checked_at: datetime!(2026-10-25 06:00 UTC),
        payer_gross: None,
        stored_gross: AiDecimal::ZERO,
        matches: None,
        error: Some("the usage source is unavailable".to_string()),
        interrupted: false,
    });
    let result = run(input);
    assert!(result
        .residual_days
        .iter()
        .all(|day| day.status == AiResidualStatus::Incomplete));
    assert_eq!(result.totals.unknown_account, 0);
    assert_eq!(result.totals.incomplete_import, 15 * CREDIT);
}

fn charge(
    day: Date,
    product: &str,
    sku: &str,
    unit: &str,
    repository: &str,
    kind: AiOrgChargeKind,
    quantity: &str,
) -> AiMeteredOrgCharge {
    AiMeteredOrgCharge {
        day,
        product: product.to_string(),
        sku: sku.to_string(),
        unit: unit.to_string(),
        repository: repository.to_string(),
        kind,
        unit_price: dec("0.01"),
        quantity: dec(quantity),
        gross_amount: dec(quantity),
        discount_amount: AiDecimal::ZERO,
        net_amount: dec(quantity),
        reported_at: None,
    }
}

#[test]
fn repositories_the_code_review_check_and_the_pool() {
    let mut input = input(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![row(
        1001,
        date!(2026 - 10 - 02),
        CREDITS_SKU,
        "Code Review model",
        AiCreditKind::RepositoryBilled,
        "100",
    )];
    input.org_charges = vec![
        charge(
            date!(2026 - 10 - 02),
            "copilot",
            CREDITS_SKU,
            "AICredits",
            "widget-api",
            AiOrgChargeKind::RepositoryCredits,
            "60",
        ),
        charge(
            date!(2026 - 10 - 03),
            "copilot",
            CREDITS_SKU,
            "AICredits",
            "widget-api",
            AiOrgChargeKind::RepositoryCredits,
            "40",
        ),
        charge(
            date!(2026 - 10 - 02),
            "copilot",
            CREDITS_SKU,
            "AICredits",
            "",
            AiOrgChargeKind::AccountCreditsAggregate,
            "900",
        ),
        charge(
            date!(2026 - 10 - 02),
            "actions",
            "Actions Linux",
            "Minutes",
            "gadget-web",
            AiOrgChargeKind::Compute,
            "120",
        ),
    ];
    let credits_line = AiMeteredSummaryLine {
        product: "copilot".to_string(),
        sku: "copilot_ai_unit".to_string(),
        unit: "ai-units".to_string(),
        kind: AiSummaryKind::Credits,
        amounts: AiMeteredAmounts {
            unit_price: dec("0.01"),
            gross_quantity: dec("2000"),
            gross_amount: dec("20"),
            discount_quantity: dec("1900"),
            discount_amount: dec("19"),
            net_quantity: dec("100"),
            net_amount: dec("1"),
        },
    };
    let seats_line = AiMeteredSummaryLine {
        sku: "copilot_for_business".to_string(),
        unit: "user-months".to_string(),
        kind: AiSummaryKind::Seats,
        amounts: AiMeteredAmounts {
            unit_price: dec("19"),
            gross_quantity: dec("2"),
            gross_amount: dec("38"),
            net_quantity: dec("2"),
            net_amount: dec("38"),
            ..AiMeteredAmounts::default()
        },
        ..credits_line.clone()
    };
    let mystery = AiMeteredSummaryLine {
        sku: "something_new".to_string(),
        kind: AiSummaryKind::Unrecognized,
        ..credits_line.clone()
    };
    input.summary = Some((
        vec![credits_line, seats_line, mystery],
        datetime!(2026-10-25 06:00 UTC),
    ));
    let mut first = org(date!(2026 - 10 - 02), "GPT-6.1 Sol", "10");
    first.credit.amounts.net_quantity = AiDecimal::ZERO;
    let mut later = org(date!(2026 - 10 - 03), "GPT-6.1 Sol", "10");
    later.credit.amounts.net_quantity = dec("0.5");
    input.org_credits = vec![first, later];

    let result = run(input);
    let widget = result
        .repositories
        .iter()
        .find(|usage| usage.repository == "widget-api")
        .unwrap();
    assert_eq!(widget.quantity, dec("100"));
    assert_eq!(widget.credit_usd, Some(usd(100 * CREDIT)));
    assert_eq!(widget.days, 2);
    let aggregate = result
        .repositories
        .iter()
        .find(|usage| usage.kind == AiOrgChargeKind::AccountCreditsAggregate)
        .unwrap();
    assert_eq!(aggregate.credit_usd, Some(usd(900 * CREDIT)));
    let actions = result
        .repositories
        .iter()
        .find(|usage| usage.kind == AiOrgChargeKind::Compute)
        .unwrap();
    assert_eq!(actions.credit_usd, None);
    // The IDE aggregate is never added to accounts.
    assert_eq!(result.totals.gross, usd(100 * CREDIT));
    assert_eq!(
        result.code_review,
        Some(AiCodeReviewCheck {
            by_accounts: usd(100 * CREDIT),
            by_repositories: usd(100 * CREDIT),
        })
    );
    let pool = result.pool.unwrap();
    assert_eq!(pool.exhausted_on, Some(date!(2026 - 10 - 03)));
    assert_eq!(pool.credits.discount_quantity, dec("1900"));
    assert_eq!(pool.seats.unwrap().gross_amount, dec("38"));
    // 1 (credits) + 38 (seats) + 1 (the unrecognized row).
    assert_eq!(pool.net_total, dec("40"));
    assert_eq!(pool.unrecognized.len(), 1);
}

#[test]
fn quiet_accounts_are_not_listed() {
    let mut input = input(vec![
        linked(1001, "ada-lovelace", ADA),
        account(1006, "long-gone", false, &[]),
    ]);
    input.coverage = BTreeMap::new();
    let result = run(input);
    // Ada holds a seat this month; the departed account has nothing.
    let logins: Vec<&str> = result
        .accounts
        .iter()
        .map(|account| account.login.as_str())
        .collect();
    assert_eq!(logins, ["ada-lovelace"]);
    assert!(result.residual_days.is_empty());
    assert_eq!(pool_exhausted_on(&[]), None);
}
