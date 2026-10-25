//! Invented fixtures for the metered reconciliation and billing tests: Ada
//! Lovelace, Grace Hopper and Alan Turing, their accounts and October 2026.

use std::collections::{BTreeMap, HashMap};

use time::{
    macros::{date, datetime},
    Date,
};

use super::{
    AiAccountDisposition, AiAccountDispositionId, AiAccountDispositionTerms, AiAccountPeriod,
    AiAccountTarget, AiBillingMonth, AiBillingProject, AiCreditKind, AiDayCoverage, AiDecimal,
    AiMappedProject, AiMeteredAmounts, AiMeteredCredit, AiMeteredDestination, AiMeteredModel,
    AiMeteredOrgCharge, AiOrgChargeKind, AiProvider, AiProviderAccountId, AiProviderLogin,
    AiReconciliationInput, AiRepositoryResolver, AiStoredAccountCredit, AiStoredOrgCredit,
    AiStoredProviderAccount, AiStoredRepositoryMapping, AiStoredSeat, AiTokenCounts,
    AiTrackedModelDay, AiUnassignedKind, AiUsageCheck, AiUsdNanos, ProjectId, TimeTrackingCompany,
    UserId,
};

pub const TODAY: Date = date!(2026 - 10 - 25);
pub const ADA: UserId = UserId::new(1);
pub const GRACE: UserId = UserId::new(2);
pub const ALAN: UserId = UserId::new(3);
pub const CREDITS_SKU: &str = "Copilot AI Credits";
pub const AGENT_SKU: &str = "Copilot Cloud Agent";
/// Nanodollars per credit.
pub const CREDIT: i64 = 10_000_000;

pub fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
}

pub fn usd(nanos: i64) -> AiUsdNanos {
    AiUsdNanos::new(nanos).unwrap()
}

pub fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap()
}

pub fn account_id(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

pub fn seat_since(day: Date) -> AiStoredSeat {
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
pub fn account(
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

pub fn linked(id: i64, login: &str, user: UserId) -> AiStoredProviderAccount {
    account(
        id,
        login,
        true,
        &[(AiAccountTarget::User(user), date!(2026 - 09 - 01), None)],
    )
}

pub fn metered_credit(
    sku: &str,
    label: &str,
    kind: AiCreditKind,
    quantity: &str,
) -> AiMeteredCredit {
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

pub fn row(
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
pub fn credits(id: i64, day: Date, label: &str, quantity: &str) -> AiStoredAccountCredit {
    row(
        id,
        day,
        CREDITS_SKU,
        label,
        AiCreditKind::Trackable,
        quantity,
    )
}

pub fn org(day: Date, label: &str, quantity: &str) -> AiStoredOrgCredit {
    let credit = metered_credit(CREDITS_SKU, label, AiCreditKind::Trackable, quantity);
    AiStoredOrgCredit {
        day,
        gross_usd: credit.gross_usd().unwrap(),
        credit,
    }
}

/// The client project most fixtures bill to.
pub fn widget() -> AiMappedProject {
    AiMappedProject {
        id: ProjectId::new("101"),
        name: "Example Client - Widget API".to_string(),
    }
}

/// Another client's project.
pub fn portal() -> AiMappedProject {
    AiMappedProject {
        id: ProjectId::new("202"),
        name: "Other Client - Portal".to_string(),
    }
}

pub fn to_project(project: AiMappedProject) -> AiMeteredDestination {
    AiMeteredDestination::Project(AiBillingProject::Mapped(project))
}

pub fn to_unassigned(kind: AiUnassignedKind) -> AiMeteredDestination {
    AiMeteredDestination::Project(AiBillingProject::Unassigned(kind))
}

/// Tracked usage of `model` on `day` to the widget project.
pub fn tracked(user: UserId, day: Date, model: &str, nanos: i64) -> AiTrackedModelDay {
    tracked_to(user, day, model, to_project(widget()), nanos)
}

/// Tracked usage of `model` on `day` to `destination`.
pub fn tracked_to(
    user: UserId,
    day: Date,
    model: &str,
    destination: AiMeteredDestination,
    nanos: i64,
) -> AiTrackedModelDay {
    AiTrackedModelDay {
        user_id: user,
        day,
        model: model.to_string(),
        destination,
        tokens: AiTokenCounts {
            input: 100,
            cache_read: 1_000,
            cache_write: 10,
            output: 50,
        },
        priced_cost: usd(nanos),
        records: 10,
        unpriced_records: 0,
        old_basis_cost: AiUsdNanos::ZERO,
        old_basis_buckets: 0,
        before_floor_cost: AiUsdNanos::ZERO,
    }
}

/// Every day of October through today, complete and fetched.
pub fn full_coverage() -> BTreeMap<Date, AiDayCoverage> {
    let mut coverage = BTreeMap::new();
    let mut day = october().first_day();
    while day <= TODAY {
        coverage.insert(
            day,
            AiDayCoverage {
                org_fetched: true,
                org_complete: day < TODAY,
                org_usage_fetched: true,
                org_usage_complete: day < TODAY,
                accounts_missing: 0,
                accounts_incomplete: u32::from(day == TODAY),
            },
        );
        day = day.next_day().unwrap();
    }
    coverage
}

/// Every account's slice of every covered day, complete before today.
pub fn all_slices(
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
pub fn org_check() -> AiUsageCheck {
    AiUsageCheck {
        account_id: None,
        checked_at: datetime!(2026-10-25 06:00 UTC),
        payer_gross: Some(dec("35")),
        stored_gross: dec("35"),
        matches: Some(true),
        error: None,
    }
}

pub fn input(accounts: Vec<AiStoredProviderAccount>) -> AiReconciliationInput {
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
        repository_mappings: AiRepositoryResolver::default(),
    }
}

/// A day after October has ended and settled.
pub const CLOSED: Date = date!(2026 - 11 - 10);

/// An organization credits row of `sku`, `label` and `kind`.
pub fn org_row(
    day: Date,
    sku: &str,
    label: &str,
    kind: AiCreditKind,
    quantity: &str,
) -> AiStoredOrgCredit {
    let credit = metered_credit(sku, label, kind, quantity);
    AiStoredOrgCredit {
        day,
        gross_usd: credit.gross_usd().unwrap(),
        credit,
    }
}

/// An account's check, matching the payer's month total.
pub fn account_check(id: i64) -> AiUsageCheck {
    AiUsageCheck {
        account_id: Some(account_id(id)),
        ..org_check()
    }
}

/// October after it settled: every day imported and complete, and every
/// stored month confirmed by the payer's month total.
pub fn closed_input(accounts: Vec<AiStoredProviderAccount>) -> AiReconciliationInput {
    let mut coverage = BTreeMap::new();
    let mut day = october().first_day();
    while october().contains(day) {
        coverage.insert(
            day,
            AiDayCoverage {
                org_fetched: true,
                org_complete: true,
                org_usage_fetched: true,
                org_usage_complete: true,
                accounts_missing: 0,
                accounts_incomplete: 0,
            },
        );
        day = day.next_day().unwrap();
    }
    let account_slices = accounts
        .iter()
        .flat_map(|account| {
            coverage
                .keys()
                .map(move |day| ((account.account_id, *day), true))
        })
        .collect();
    let checks = std::iter::once(org_check())
        .chain(
            accounts
                .iter()
                .map(|account| account_check(account.account_id.as_i64())),
        )
        .collect();
    AiReconciliationInput {
        account_slices,
        today: CLOSED,
        checks,
        coverage,
        ..input(accounts)
    }
}

/// An organization usage row of `product`, `sku` and `unit` on `repository`,
/// at $0.01 a unit.
pub fn org_charge(
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

/// Code review credits the organization's rows bill to `repository`.
pub fn repository_credits(day: Date, repository: &str, quantity: &str) -> AiMeteredOrgCharge {
    org_charge(
        day,
        "copilot",
        CREDITS_SKU,
        "AICredits",
        repository,
        AiOrgChargeKind::RepositoryCredits,
        quantity,
    )
}

/// The time-tracking company mappings resolve to.
pub fn company() -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: "company-1".to_string(),
    }
}

/// A mapping of `repository` to `project` of `company`.
pub fn repository_mapping(
    repository: &str,
    company: TimeTrackingCompany,
    project: AiMappedProject,
) -> AiStoredRepositoryMapping {
    AiStoredRepositoryMapping {
        repository: repository.to_string(),
        company,
        project,
    }
}
