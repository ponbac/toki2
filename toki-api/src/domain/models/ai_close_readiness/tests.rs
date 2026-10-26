use time::{
    macros::{date, datetime},
    UtcOffset,
};

use super::*;
use crate::domain::models::{
    ai_metered_fixtures::{
        account, account_check, closed_input, company, credits, linked, org, org_charge, org_check,
        repository_credits, repository_mapping, tracked, widget, ADA, CREDIT, GRACE,
    },
    allocate_metered_credits, bill_month, convert_bill, AiAccountTarget, AiAdjustedKind,
    AiAdjustedMonth, AiAdjustmentKey, AiAdjustmentLine, AiBillingMachine, AiBillingProject,
    AiBillingSubject, AiBillingUsage, AiConvertedAmount, AiCurrency, AiDailyUsage,
    AiDeveloperCompleteness, AiDeveloperReadiness, AiExchangeRate, AiExchangeRateOverride,
    AiExchangeRateValue, AiFeeAmount, AiFetchedExchangeRate, AiImportKind, AiImportRun,
    AiImportRunId, AiKeyPayer, AiKeyProject, AiKeySubject, AiMachineCompleteness, AiMonthCloseId,
    AiMonthMode, AiOrgChargeKind, AiProviderCompleteness, AiRepositoryResolver, AiRunStatus,
    AiUsageCheck,
};

/// October settled and checked after it settled (4 November).
fn settled(accounts: Vec<crate::domain::models::AiStoredProviderAccount>) -> AiReconciliationInput {
    let mut input = closed_input(accounts);
    for check in &mut input.checks {
        check.checked_at = datetime!(2026-11-05 06:00 UTC);
    }
    input
}

fn checks_of(input: &AiReconciliationInput) -> [AiCloseCheck; 4] {
    let allocation = allocate_metered_credits(input).unwrap();
    metered_checks(input, &allocation, None)
}

fn problems(check: &AiCloseCheck) -> &[AiCloseProblem] {
    assert_eq!(
        check.status == AiCloseCheckStatus::Failed,
        !check.problems.is_empty()
    );
    &check.problems
}

/// Ada linked and seated, with 10 credits on 5 October and tracked usage
/// enough to attribute them, the organization agreeing.
fn ada_october() -> AiReconciliationInput {
    let day = date!(2026 - 10 - 05);
    let mut input = settled(vec![linked(1001, "ada-lovelace", ADA)]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "10")];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT)];
    input
}

const IMPORT_REASONS: [AiMeteredProvisional; 3] = [
    AiMeteredProvisional::MonthInProgress,
    AiMeteredProvisional::ImportIncomplete,
    AiMeteredProvisional::NotSettled,
];

#[test]
fn a_settled_imported_and_checked_month_passes_every_metered_check() {
    let input = ada_october();
    let allocation = allocate_metered_credits(&input).unwrap();
    let checks = metered_checks(&input, &allocation, None);
    for check in &checks {
        assert_eq!(check.status, AiCloseCheckStatus::Passed, "{check:?}");
    }
    // Passing the import check means nothing may still change for the
    // import's sake.
    assert!(allocation
        .lines
        .iter()
        .flat_map(|line| &line.charge.provisional)
        .chain(&allocation.unknown_account)
        .chain(&allocation.repository_provisional)
        .all(|reason| !IMPORT_REASONS.contains(reason)));
    assert_eq!(
        checks.iter().map(|check| check.id).collect::<Vec<_>>(),
        AiCloseCheckId::ALL[..4]
    );
}

#[test]
fn the_import_waits_for_the_settle_days() {
    let mut input = ada_october();
    input.today = date!(2026 - 11 - 03);
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import)[0],
        AiCloseProblem::NotSettled {
            last_day: date!(2026 - 10 - 31),
            settles_on: date!(2026 - 11 - 04),
            settle_days: 3,
        }
    );
    // A longer measured lag moves it.
    input.settle_days = 5;
    input.today = date!(2026 - 11 - 05);
    let [import, ..] = checks_of(&input);
    assert!(matches!(
        problems(&import)[0],
        AiCloseProblem::NotSettled {
            settles_on,
            settle_days: 5,
            ..
        } if settles_on == date!(2026 - 11 - 06)
    ));
    // Settled, but the checks of the 5th predate it.
    input.today = date!(2026 - 11 - 06);
    let [import, ..] = checks_of(&input);
    assert!(problems(&import).iter().all(|problem| matches!(
        problem,
        AiCloseProblem::AccountCheck {
            outcome: AiCheckOutcome::BeforeSettled { .. },
            ..
        } | AiCloseProblem::OrgCheck {
            outcome: AiCheckOutcome::BeforeSettled { .. }
        }
    )));
    for check in &mut input.checks {
        check.checked_at = datetime!(2026-11-06 00:00 UTC);
    }
    let [import, ..] = checks_of(&input);
    assert_eq!(import.status, AiCloseCheckStatus::Passed);
}

#[test]
fn checks_made_before_the_month_settled_do_not_count() {
    let mut input = ada_october();
    for check in &mut input.checks {
        check.checked_at = datetime!(2026-11-03 23:59 UTC);
    }
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [
            AiCloseProblem::AccountCheck {
                account_id: AiProviderAccountId::new(1001).unwrap(),
                login: AiProviderLogin::parse("ada-lovelace").unwrap(),
                outcome: AiCheckOutcome::BeforeSettled {
                    checked_at: datetime!(2026-11-03 23:59 UTC),
                },
            },
            AiCloseProblem::OrgCheck {
                outcome: AiCheckOutcome::BeforeSettled {
                    checked_at: datetime!(2026-11-03 23:59 UTC),
                },
            },
        ]
    );
}

#[test]
fn missing_and_incomplete_slices_fail_per_source() {
    let mut input = ada_october();
    let cover = input.coverage.get_mut(&date!(2026 - 10 - 12)).unwrap();
    cover.accounts_missing = 1;
    cover.org_usage_complete = false;
    let cover = input.coverage.get_mut(&date!(2026 - 10 - 14)).unwrap();
    cover.accounts_incomplete = 1;
    cover.org_fetched = false;
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [
            AiCloseProblem::DaysNotImported {
                source: AiMeteredSource::AccountCredits,
                days: 2,
                first: date!(2026 - 10 - 12),
                last: date!(2026 - 10 - 14),
            },
            AiCloseProblem::DaysNotImported {
                source: AiMeteredSource::OrgCredits,
                days: 1,
                first: date!(2026 - 10 - 14),
                last: date!(2026 - 10 - 14),
            },
            AiCloseProblem::DaysNotImported {
                source: AiMeteredSource::OrgUsage,
                days: 1,
                first: date!(2026 - 10 - 12),
                last: date!(2026 - 10 - 12),
            },
        ]
    );
}

#[test]
fn days_before_the_backfill_are_not_imported() {
    let mut input = ada_october();
    for day in 1..=3 {
        input
            .coverage
            .remove(&date!(2026 - 10 - 01).replace_day(day).unwrap());
    }
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [AiCloseProblem::DaysBeforeBackfill {
            first: date!(2026 - 10 - 01),
            last: date!(2026 - 10 - 03),
        }]
    );
}

fn failed(account: Option<i64>, error: &str) -> AiUsageCheck {
    AiUsageCheck {
        account_id: account.and_then(AiProviderAccountId::new),
        checked_at: datetime!(2026-11-05 06:00 UTC),
        payer_gross: None,
        stored_gross: crate::domain::models::AiDecimal::ZERO,
        matches: None,
        error: Some(error.to_string()),
        interrupted: false,
    }
}

/// The check a run that stopped on 2 November left on `account`'s (or the
/// organization's) month.
fn interrupted(account: Option<i64>) -> AiUsageCheck {
    AiUsageCheck::interrupted(
        account.and_then(AiProviderAccountId::new),
        datetime!(2026-11-02 02:00 UTC),
        crate::domain::models::AiDecimal::ZERO,
        "GitHub's rate limiting stopped the import",
    )
}

const STOPPED: &str =
    "the run stopped before finishing this month: GitHub's rate limiting stopped the import";

#[test]
fn a_month_a_stopped_run_left_interrupted_is_not_ready_on_import() {
    let day = date!(2026 - 10 - 05);
    let mut input = settled(vec![
        linked(1001, "ada-lovelace", ADA),
        // No seat and no credits stored: its own failure would be excused,
        // but the run may have deleted its days before stopping.
        account(1004, "retired-account", false, &[]),
    ]);
    input.account_credits = vec![credits(1001, day, "GPT-6.1 Sol", "10")];
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "10")];
    input.tracked = vec![tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT)];
    let set_check = |input: &mut AiReconciliationInput, check: AiUsageCheck| {
        for stored in &mut input.checks {
            if stored.account_id == check.account_id {
                *stored = check.clone();
            }
        }
    };
    set_check(&mut input, failed(Some(1004), "GitHub answered 404"));
    let [import, ..] = checks_of(&input);
    assert_eq!(import.status, AiCloseCheckStatus::Passed);

    set_check(&mut input, interrupted(Some(1004)));
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [AiCloseProblem::AccountCheck {
            account_id: AiProviderAccountId::new(1004).unwrap(),
            login: AiProviderLogin::parse("retired-account").unwrap(),
            outcome: AiCheckOutcome::Interrupted(STOPPED.to_string()),
        }]
    );

    // Stopped on the organization's credits, before any account.
    for check in &mut input.checks {
        *check = interrupted(check.account_id.map(AiProviderAccountId::as_i64));
    }
    let [import, ..] = checks_of(&input);
    let outcome = AiCheckOutcome::Interrupted(STOPPED.to_string());
    assert_eq!(
        problems(&import),
        [
            AiCloseProblem::AccountCheck {
                account_id: AiProviderAccountId::new(1001).unwrap(),
                login: AiProviderLogin::parse("ada-lovelace").unwrap(),
                outcome: outcome.clone(),
            },
            AiCloseProblem::AccountCheck {
                account_id: AiProviderAccountId::new(1004).unwrap(),
                login: AiProviderLogin::parse("retired-account").unwrap(),
                outcome: outcome.clone(),
            },
            AiCloseProblem::OrgCheck { outcome },
        ]
    );
    assert!(!AiCloseReadiness {
        month: input.month,
        evaluated_at: datetime!(2026-11-05 07:00 UTC),
        checks: vec![import],
    }
    .ready());
}

#[test]
fn a_live_run_importing_the_month_fails_the_import() {
    let input = ada_october();
    let allocation = allocate_metered_credits(&input).unwrap();
    let run = |months: &[&str]| AiImportRun {
        id: AiImportRunId::new(7),
        kind: AiImportKind::Hourly,
        months: months
            .iter()
            .map(|month| (AiBillingMonth::parse(month).unwrap(), AiMonthMode::Changes))
            .collect(),
        started_at: datetime!(2026-11-05 07:00 UTC),
        finished_at: None,
        status: AiRunStatus::Running,
        requests: 0,
        slices_fetched: 0,
        slices_changed: 0,
        account_errors: 0,
        error: None,
        sources: Vec::new(),
    };
    // Its marks keep the checks they marked, which still match: the run is
    // the problem, as it may be replacing October's days right now.
    let [import, ..] = metered_checks(&input, &allocation, Some(&run(&["2026-10", "2026-11"])));
    assert_eq!(
        problems(&import),
        [AiCloseProblem::ImportRunning {
            started_at: datetime!(2026-11-05 07:00 UTC),
        }]
    );
    // A run importing only November leaves October as it is.
    let [import, ..] = metered_checks(&input, &allocation, Some(&run(&["2026-11"])));
    assert_eq!(import.status, AiCloseCheckStatus::Passed);
}

#[test]
fn every_seated_account_and_every_account_with_credits_needs_a_matching_check() {
    let day = date!(2026 - 10 - 05);
    let mut input = settled(vec![
        // Seated, linked: its check is missing.
        linked(1001, "ada-lovelace", ADA),
        // Seated, its check failed.
        linked(1002, "grace-hopper", GRACE),
        // No seat, but credits: its check differs.
        account(
            1003,
            "alan-turing",
            false,
            &[(AiAccountTarget::External, date!(2026 - 09 - 01), None)],
        ),
        // No seat, no credits, a failed import: excused.
        account(1004, "retired-account", false, &[]),
    ]);
    input.account_credits = vec![credits(1003, day, "GPT-6.1 Sol", "4")];
    input.org_credits = vec![org(day, "GPT-6.1 Sol", "4")];
    input.checks = vec![
        AiUsageCheck {
            checked_at: datetime!(2026-11-05 06:00 UTC),
            ..org_check()
        },
        failed(Some(1002), "GitHub answered 502"),
        AiUsageCheck {
            matches: Some(false),
            checked_at: datetime!(2026-11-05 06:00 UTC),
            ..account_check(1003)
        },
        failed(Some(1004), "GitHub answered 404"),
    ];
    let [import, ..] = checks_of(&input);
    let outcomes: Vec<(String, AiCheckOutcome)> = problems(&import)
        .iter()
        .filter_map(|problem| match problem {
            AiCloseProblem::AccountCheck { login, outcome, .. } => {
                Some((login.as_str().to_string(), outcome.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            ("ada-lovelace".to_string(), AiCheckOutcome::Missing),
            ("alan-turing".to_string(), AiCheckOutcome::Differs),
            (
                "grace-hopper".to_string(),
                AiCheckOutcome::Failed("GitHub answered 502".to_string())
            ),
        ]
    );
}

#[test]
fn the_organizations_check_must_match_too() {
    let mut input = ada_october();
    input.checks.retain(|check| check.account_id.is_some());
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [AiCloseProblem::OrgCheck {
            outcome: AiCheckOutcome::Missing,
        }]
    );
    input.checks.push(failed(None, "timeout"));
    let [import, ..] = checks_of(&input);
    assert_eq!(
        problems(&import),
        [AiCloseProblem::OrgCheck {
            outcome: AiCheckOutcome::Failed("timeout".to_string()),
        }]
    );
}

#[test]
fn credits_of_an_unlinked_account_fail_dispositions() {
    let day = date!(2026 - 10 - 05);
    let mut input = settled(vec![account(1005, "unlinked-dev", true, &[])]);
    input.account_credits = vec![
        credits(1005, day, "GPT-6.1 Sol", "7"),
        credits(1005, date!(2026 - 10 - 06), "GPT-5.4", "3"),
    ];
    input.org_credits = vec![
        org(day, "GPT-6.1 Sol", "7"),
        org(date!(2026 - 10 - 06), "GPT-5.4", "3"),
    ];
    let [_, dispositions, ..] = checks_of(&input);
    assert_eq!(
        problems(&dispositions),
        [AiCloseProblem::UnlinkedCredits {
            account_id: AiProviderAccountId::new(1005).unwrap(),
            login: AiProviderLogin::parse("unlinked-dev").unwrap(),
            credits: 10,
        }]
    );
}

#[test]
fn the_unknown_accounts_residual_must_be_settled_and_zero() {
    let input = ada_october();
    let [_, _, unknown, _] = checks_of(&input);
    assert_eq!(unknown.status, AiCloseCheckStatus::Passed);

    // Accounts Toki never listed billed 5 credits on the 7th.
    let mut more = ada_october();
    more.org_credits
        .push(org(date!(2026 - 10 - 07), "GPT-6.1 Sol", "5"));
    let [_, _, unknown, _] = checks_of(&more);
    assert_eq!(
        problems(&unknown),
        [AiCloseProblem::Residual {
            day: date!(2026 - 10 - 07),
            nanos: 5 * CREDIT,
        }]
    );

    // Accounts above the organization: a negative residual fails too.
    let mut less = ada_october();
    less.org_credits = vec![org(date!(2026 - 10 - 05), "GPT-6.1 Sol", "8")];
    let [_, _, unknown, _] = checks_of(&less);
    assert_eq!(
        problems(&unknown),
        [AiCloseProblem::Residual {
            day: date!(2026 - 10 - 05),
            nanos: -2 * CREDIT,
        }]
    );

    // An organization slice not complete leaves the residual unsettled.
    let mut unsettled = ada_october();
    unsettled
        .coverage
        .get_mut(&date!(2026 - 10 - 09))
        .unwrap()
        .org_complete = false;
    let [_, _, unknown, _] = checks_of(&unsettled);
    assert_eq!(
        problems(&unknown),
        [AiCloseProblem::ResidualNotSettled {
            reasons: vec![AiMeteredProvisional::NotSettled],
        }]
    );
}

#[test]
fn every_repository_with_credits_must_be_mapped() {
    let day = date!(2026 - 10 - 05);
    let mut input = ada_october();
    input.org_charges = vec![
        repository_credits(day, "Widget-API", "12"),
        repository_credits(day, "gadget-web", "8"),
        // Under half a credit rounds to no line: nothing to map.
        repository_credits(day, "tiny-tool", "0.004"),
    ];
    input.repository_mappings = AiRepositoryResolver::new(
        vec![repository_mapping("widget-api", company(), widget())],
        Some(company()),
    );
    let [.., repositories] = checks_of(&input);
    assert_eq!(
        problems(&repositories),
        [AiCloseProblem::UnmappedRepository {
            repository: "gadget-web".to_string(),
            credits: 8,
        }]
    );

    // An unrecognized row naming a repository may be code review on no line.
    input.org_charges = vec![
        repository_credits(day, "Widget-API", "12"),
        org_charge(
            day,
            "copilot",
            "Copilot Review Premium",
            "AICredits",
            "widget-api",
            AiOrgChargeKind::Unrecognized,
            "3",
        ),
    ];
    let [.., repositories] = checks_of(&input);
    assert_eq!(
        problems(&repositories),
        [AiCloseProblem::UnrecognizedRepositoryRow]
    );
}

#[test]
fn pricing_fails_per_account_and_per_machine() {
    let day = date!(2026 - 10 - 05);
    let mut input = settled(vec![
        linked(1001, "ada-lovelace", ADA),
        linked(1002, "grace-hopper", GRACE),
    ]);
    input.account_credits = vec![
        credits(1001, day, "GPT-6.1 Sol", "10"),
        credits(1002, day, "GPT-6 Astra", "10"),
        credits(1002, day, "Brand New Model", "1"),
    ];
    input.org_credits = vec![
        org(day, "GPT-6.1 Sol", "10"),
        org(day, "GPT-6 Astra", "10"),
        org(day, "Brand New Model", "1"),
    ];
    let mut old = tracked(ADA, day, "gpt-6.1-sol", 10 * CREDIT);
    old.old_basis_buckets = 2;
    old.old_basis_cost = AiUsdNanos::new(10 * CREDIT).unwrap();
    input.tracked = vec![
        old,
        tracked(GRACE, day, "gpt-6-astra", 10 * CREDIT),
        tracked(GRACE, day, "brand-new-model", CREDIT),
    ];
    let allocation = allocate_metered_credits(&input).unwrap();
    let machine = AiOldBasisUsage {
        developer: AiBillingDeveloper {
            user_id: GRACE,
            full_name: "Grace Hopper".to_string(),
            email: "grace@example.com".to_string(),
        },
        machine_id: AiMachineId::parse("7e57c0de-0000-4000-8000-000000000002").unwrap(),
        label: "retired-laptop".to_string(),
        provider: AiProvider::Claude,
        buckets: 12,
        first_day: date!(2026 - 10 - 02),
        last_day: date!(2026 - 10 - 09),
        estimate: AiUsdNanos::new(3_500_000_000).unwrap(),
    };
    let check = pricing_check(Some(&allocation), std::slice::from_ref(&machine));
    // Grace's unrecognized model never blocks: GitHub's credits are its own
    // number.
    assert_eq!(
        problems(&check),
        [
            AiCloseProblem::MeteredPricing {
                account_id: AiProviderAccountId::new(1001).unwrap(),
                login: AiProviderLogin::parse("ada-lovelace").unwrap(),
                reasons: vec![AiMeteredProvisional::OldPricingBasis],
            },
            AiCloseProblem::MeteredPricing {
                account_id: AiProviderAccountId::new(1002).unwrap(),
                login: AiProviderLogin::parse("grace-hopper").unwrap(),
                reasons: vec![AiMeteredProvisional::ProvisionalModel],
            },
            AiCloseProblem::OldBasis {
                developer: machine.developer.clone(),
                machine_id: machine.machine_id,
                label: "retired-laptop".to_string(),
                provider: AiProvider::Claude,
                buckets: 12,
                first_day: date!(2026 - 10 - 02),
                last_day: date!(2026 - 10 - 09),
                estimate: machine.estimate,
            },
        ]
    );
    assert_eq!(check.id, AiCloseCheckId::Pricing);
    // Estimates alone, before the cutover.
    assert_eq!(pricing_check(None, &[]).status, AiCloseCheckStatus::Passed);
}

fn usd() -> AiCurrency {
    AiCurrency::usd()
}

fn fetched(month: AiBillingMonth, raw: &str, provisional: bool) -> AiExchangeRate {
    AiExchangeRate {
        month,
        currency: usd(),
        fetched: Some(AiFetchedExchangeRate {
            rate: AiExchangeRateValue::parse(raw).unwrap(),
            provider: "riksbank".to_string(),
            provisional,
            observed_through: date!(2026 - 10 - 28),
            fetched_at: datetime!(2026-10-29 08:00 UTC),
            fetched_on: date!(2026 - 10 - 29),
        }),
        overridden: None,
    }
}

/// October with 1.25 USD of Ada's API usage.
fn api_bill() -> AiMonthBill {
    bill_month(
        october(),
        &[AiDailyUsage {
            billing_provider: None,
            user_id: ADA,
            provider: AiProvider::Claude,
            day: date!(2026 - 10 - 05),
            chatgpt_sign_in: false,
            project: AiBillingProject::Mapped(widget()),
            usage: AiBillingUsage {
                records: 1,
                priced_cost: AiUsdNanos::new(1_250_000_000).unwrap(),
                ..AiBillingUsage::default()
            },
        }],
        &[],
    )
    .unwrap()
}

fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
}

#[test]
fn every_rate_must_be_stored_fetched_and_final() {
    let bill = api_bill();
    let rates = |rates: &[AiExchangeRate], pending: &[AiCurrency]| {
        exchange_rate_check(&bill, &convert_bill(&bill, rates, pending).unwrap())
    };
    assert_eq!(
        problems(&rates(&[], &[])),
        [AiCloseProblem::RateMissing { currency: usd() }]
    );
    assert_eq!(
        problems(&rates(&[], &[usd()])),
        [AiCloseProblem::RatePending { currency: usd() }]
    );
    assert_eq!(
        problems(&rates(&[fetched(october(), "10.5", true)], &[])),
        [AiCloseProblem::RateProvisional {
            currency: usd(),
            observed_through: date!(2026 - 10 - 28),
        }]
    );
    assert_eq!(
        rates(&[fetched(october(), "10.5", false)], &[]).status,
        AiCloseCheckStatus::Passed
    );
    // An admin's override is final.
    let mut overridden = fetched(october(), "10.5", true);
    overridden.overridden = Some(AiExchangeRateOverride {
        rate: AiExchangeRateValue::parse("10.75").unwrap(),
        set_on: date!(2026 - 11 - 02),
        by: None,
    });
    assert_eq!(rates(&[overridden], &[]).status, AiCloseCheckStatus::Passed);
}

fn adjustment(month: AiBillingMonth, converted: AiConvertedAmount) -> AiAdjustmentLine {
    AiAdjustmentLine {
        adjusts: AiAdjustedMonth {
            month,
            close_id: AiMonthCloseId::new(3),
        },
        key: AiAdjustmentKey {
            subject: AiKeySubject::User(ADA),
            project: AiKeyProject::Project(widget().id),
            kind: AiAdjustedKind::Api,
            currency: usd(),
            payer: AiKeyPayer::Provider(AiProvider::Claude),
        },
        subject: AiBillingSubject::User(ADA),
        provider: AiProvider::Claude,
        project: Some(widget()),
        unassigned: None,
        plan: None,
        amount: AiFeeAmount {
            hundredths: -125,
            currency: usd(),
        },
        converted,
        credits: None,
        rate_changed: false,
    }
}

#[test]
fn the_rates_of_corrected_months_must_be_final_too() {
    let september = AiBillingMonth::parse("2026-09").unwrap();
    let mut bill = api_bill();
    bill.adjustments = vec![
        adjustment(
            september,
            AiConvertedAmount::Converted {
                amount: AiFeeAmount {
                    hundredths: -1_313,
                    currency: AiCurrency::billing(),
                },
                rate: Some(fetched(september, "10.5", true)),
            },
        ),
        adjustment(
            september,
            AiConvertedAmount::MissingRate { currency: usd() },
        ),
    ];
    let converted = convert_bill(&bill, &[fetched(october(), "10.5", false)], &[]).unwrap();
    assert_eq!(
        problems(&exchange_rate_check(&bill, &converted)),
        [
            AiCloseProblem::AdjustedRateProvisional {
                month: september,
                currency: usd(),
            },
            AiCloseProblem::AdjustedRateMissing {
                month: september,
                currency: usd(),
            },
        ]
    );
}

/// Local days of `month` at a fixed offset (no daylight saving inside).
fn local_days(month: AiBillingMonth, offset_hours: i8) -> Vec<AiLocalDay> {
    let offset = UtcOffset::from_hms(offset_hours, 0, 0).unwrap();
    let mut days = Vec::new();
    let mut day = month.first_day();
    while month.contains(day) {
        let next = day.next_day().unwrap();
        days.push(AiLocalDay {
            day,
            start: day
                .midnight()
                .assume_offset(offset)
                .to_offset(UtcOffset::UTC),
            end: next
                .midnight()
                .assume_offset(offset)
                .to_offset(UtcOffset::UTC),
        });
        day = next;
    }
    days
}

#[test]
fn the_utc_month_outside_the_local_one_is_its_tail_or_head() {
    let august = AiBillingMonth::parse("2026-08").unwrap();
    // Stockholm in summer (CEST): the UTC month ends two hours after it.
    assert_eq!(
        utc_spans_outside(august, &local_days(august, 2)),
        [(
            datetime!(2026-08-31 22:00 UTC),
            datetime!(2026-09-01 00:00 UTC)
        )]
    );
    let november = AiBillingMonth::parse("2026-11").unwrap();
    // In winter (CET), one hour.
    assert_eq!(
        utc_spans_outside(november, &local_days(november, 1)),
        [(
            datetime!(2026-11-30 23:00 UTC),
            datetime!(2026-12-01 00:00 UTC)
        )]
    );
    // West of UTC, the UTC month starts before the local one.
    assert_eq!(
        utc_spans_outside(november, &local_days(november, -5)),
        [(
            datetime!(2026-11-01 00:00 UTC),
            datetime!(2026-11-01 05:00 UTC)
        )]
    );
    assert!(utc_spans_outside(november, &local_days(november, 0)).is_empty());
}

fn machine_id() -> AiMachineId {
    AiMachineId::parse("5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a").unwrap()
}

fn completeness(active: bool, complete: bool) -> AiMonthCompleteness {
    let august = AiBillingMonth::parse("2026-08").unwrap();
    let gaps = if complete {
        Vec::new()
    } else {
        vec![
            AiUsageDateRange::from_inclusive(date!(2026 - 08 - 20), date!(2026 - 08 - 21)).unwrap(),
        ]
    };
    AiMonthCompleteness {
        month: august,
        required_through: Some(august.last_day()),
        in_progress: false,
        developers: vec![AiDeveloperCompleteness {
            developer: AiBillingDeveloper {
                user_id: ADA,
                full_name: "Ada Lovelace".to_string(),
                email: "ada@example.com".to_string(),
            },
            readiness: AiDeveloperReadiness::Ready,
            has_usage: true,
            has_subscription: false,
            machines: vec![AiMachineCompleteness {
                machine: AiBillingMachine {
                    user_id: ADA,
                    machine_id: machine_id(),
                    label: "work-laptop".to_string(),
                    client_version: "0.10.0".to_string(),
                    last_synced_at: datetime!(2026-09-02 08:00 UTC),
                    last_synced_on: date!(2026 - 09 - 02),
                    synced_since_month_start: true,
                    providers_with_usage: vec![AiProvider::Copilot, AiProvider::Claude],
                    latest_reports: Vec::new(),
                    intervals: Vec::new(),
                },
                stale: false,
                active_in_month: active,
                gaps: gaps.clone(),
                providers: [AiProvider::Claude, AiProvider::Copilot]
                    .into_iter()
                    .map(|provider| AiProviderCompleteness {
                        provider,
                        latest_report: None,
                        expected: true,
                        gaps: gaps.clone(),
                    })
                    .collect(),
                complete,
            }],
        }],
    }
}

fn interval(
    provider: AiProvider,
    start: OffsetDateTime,
    end: OffsetDateTime,
) -> (AiMachineId, AiCoverageInterval) {
    (
        machine_id(),
        AiCoverageInterval {
            provider,
            start,
            end,
        },
    )
}

#[test]
fn uploads_need_every_active_machine_complete_and_the_utc_tail_when_metered() {
    let august = AiBillingMonth::parse("2026-08").unwrap();
    let spans = utc_spans_outside(august, &local_days(august, 2));
    let tail = (
        datetime!(2026-08-31 22:00 UTC),
        datetime!(2026-09-01 00:00 UTC),
    );
    let ada = completeness(true, true).developers[0].developer.clone();

    // Incomplete: its gaps, and its expected providers' gaps.
    let check = uploads_check(&completeness(true, false), false, &[], &[]);
    assert!(matches!(
        problems(&check),
        [AiCloseProblem::MachineIncomplete { providers, gaps, .. }]
            if providers.len() == 2 && gaps.len() == 1
    ));
    // An inactive machine never blocks.
    assert_eq!(
        uploads_check(&completeness(false, false), true, &spans, &[]).status,
        AiCloseCheckStatus::Passed
    );
    // Before the cutover, the UTC tail does not matter.
    assert_eq!(
        uploads_check(&completeness(true, true), false, &spans, &[]).status,
        AiCloseCheckStatus::Passed
    );
    // Metered: nothing logged after the local month ends.
    assert_eq!(
        problems(&uploads_check(&completeness(true, true), true, &spans, &[])),
        [AiCloseProblem::UtcHoursNotUploaded {
            developer: ada.clone(),
            machine_id: machine_id(),
            label: "work-laptop".to_string(),
            provider: None,
            from: tail.0,
            until: tail.1,
        }]
    );
    // Claude covers the tail, Copilot stops at 23:00: Copilot's tail is
    // missing, and the span is reported, never when the sync was.
    let partly = [
        interval(
            AiProvider::Claude,
            datetime!(2026-08-31 20:00 UTC),
            datetime!(2026-09-01 02:00 UTC),
        ),
        interval(
            AiProvider::Copilot,
            datetime!(2026-08-31 20:00 UTC),
            datetime!(2026-08-31 23:00 UTC),
        ),
    ];
    assert_eq!(
        problems(&uploads_check(
            &completeness(true, true),
            true,
            &spans,
            &partly
        )),
        [AiCloseProblem::UtcHoursNotUploaded {
            developer: ada,
            machine_id: machine_id(),
            label: "work-laptop".to_string(),
            provider: Some(AiProvider::Copilot),
            from: tail.0,
            until: tail.1,
        }]
    );
    // Two Copilot syncs covering it between them pass.
    let covered = [
        partly[0],
        partly[1],
        interval(
            AiProvider::Copilot,
            datetime!(2026-08-31 22:30 UTC),
            datetime!(2026-09-01 06:00 UTC),
        ),
    ];
    assert_eq!(
        uploads_check(&completeness(true, true), true, &spans, &covered).status,
        AiCloseCheckStatus::Passed
    );
}

fn readiness() -> AiCloseReadiness {
    let mut import = AiCloseCheck::judged(
        AiCloseCheckId::Import,
        vec![AiCloseProblem::OrgCheck {
            outcome: AiCheckOutcome::Missing,
        }],
    );
    import.waiver = None;
    AiCloseReadiness {
        month: october(),
        evaluated_at: datetime!(2026-11-05 08:00 UTC),
        checks: vec![
            import,
            AiCloseCheck::judged(AiCloseCheckId::Dispositions, Vec::new()),
            AiCloseCheck::not_applicable(AiCloseCheckId::Repositories),
        ],
    }
}

#[test]
fn a_waiver_applies_to_a_failed_check_only() {
    let reason = |raw: &str| AiWaiverReason::parse(raw).unwrap();
    assert!(!readiness().ready());
    let waived = readiness().waive(vec![
        AiCloseWaiver {
            check: AiCloseCheckId::Import,
            reason: reason("  The organization's check runs nightly; totals agree.  "),
        },
        AiCloseWaiver {
            check: AiCloseCheckId::Dispositions,
            reason: reason("not needed"),
        },
        AiCloseWaiver {
            check: AiCloseCheckId::Repositories,
            reason: reason("not needed"),
        },
    ]);
    assert!(waived.ready());
    assert_eq!(waived.waived(), [AiCloseCheckId::Import]);
    assert_eq!(
        waived.checks[0].waiver.as_ref().map(AiWaiverReason::as_str),
        Some("The organization's check runs nightly; totals agree.")
    );
    // Passed and not applicable checks keep no waiver.
    assert!(waived.checks[1..]
        .iter()
        .all(|check| check.waiver.is_none()));

    assert!(AiWaiverReason::parse("   ").is_err());
    assert!(AiWaiverReason::parse(&"x".repeat(501)).is_err());
    assert!(AiWaiverReason::parse(&"x".repeat(500)).is_ok());
    assert!(AiWaiverReason::parse("two\nlines").is_ok());
    assert!(AiWaiverReason::parse("bell\u{7}").is_err());
    assert_eq!(
        AiCloseCheckId::parse("unknownAccount"),
        Some(AiCloseCheckId::UnknownAccount)
    );
    assert_eq!(AiCloseCheckId::parse("everything"), None);
}

#[test]
fn no_waiver_lets_a_month_close_while_an_import_of_it_runs() {
    let started_at = datetime!(2026-11-05 07:58 UTC);
    let mut running = readiness();
    running.checks[0]
        .problems
        .insert(0, AiCloseProblem::ImportRunning { started_at });
    running.checks.push(AiCloseCheck::judged(
        AiCloseCheckId::ExchangeRates,
        vec![AiCloseProblem::RateMissing {
            currency: AiCurrency::usd(),
        }],
    ));
    let reason = AiWaiverReason::parse("Closing October anyway.").unwrap();
    let waivers =
        [AiCloseCheckId::Import, AiCloseCheckId::ExchangeRates].map(|check| AiCloseWaiver {
            check,
            reason: reason.clone(),
        });

    // The import keeps no waiver, whatever else it found; the other checks
    // stay waivable.
    let waived = running.waive(waivers.to_vec());
    assert!(!waived.ready());
    assert_eq!(waived.import_running(), Some(started_at));
    assert_eq!(waived.checks[0].status, AiCloseCheckStatus::Failed);
    assert_eq!(waived.checks[0].waiver, None);
    assert_eq!(waived.waived(), [AiCloseCheckId::ExchangeRates]);

    // Once the run finishes, the same waivers close the month.
    let mut finished = waived;
    finished.checks[0].problems.remove(0);
    let finished = finished.waive(waivers.to_vec());
    assert!(finished.ready());
    assert_eq!(finished.import_running(), None);
    assert_eq!(
        finished.waived(),
        [AiCloseCheckId::Import, AiCloseCheckId::ExchangeRates]
    );
}

#[test]
fn metered_checks_do_not_apply_before_the_cutover() {
    let checks = metered_not_applicable();
    assert!(checks
        .iter()
        .all(|check| check.status == AiCloseCheckStatus::NotApplicable));
}
