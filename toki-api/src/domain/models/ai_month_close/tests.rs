use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{
    bill_month, convert_bill, AiBillingLine, AiBillingProject, AiBillingUsage, AiDailyUsage,
    AiFetchedExchangeRate, AiMeteredCharge, AiMonthlyCost, AiSubscription, AiSubscriptionPeriod,
    AiSubscriptionPlan, AiSubscriptionTerms, AiUsdNanos,
};

const ADA: UserId = UserId::new(1);
const GRACE: UserId = UserId::new(2);

fn month(raw: &str) -> AiBillingMonth {
    AiBillingMonth::parse(raw).unwrap()
}

fn usd() -> AiCurrency {
    AiCurrency::usd()
}

fn rate(month: AiBillingMonth, raw: &str) -> AiExchangeRate {
    AiExchangeRate {
        month,
        currency: usd(),
        fetched: Some(AiFetchedExchangeRate {
            rate: AiExchangeRateValue::parse(raw).unwrap(),
            provider: "riksbank".to_string(),
            provisional: false,
            observed_through: month.last_day(),
            fetched_at: datetime!(2026-10-02 08:00 UTC),
            fetched_on: date!(2026 - 10 - 02),
        }),
        overridden: None,
    }
}

fn project(id: &str, name: &str) -> AiMappedProject {
    AiMappedProject {
        id: ProjectId::new(id),
        name: name.to_string(),
    }
}

fn client_a() -> AiMappedProject {
    project("101", "Client A")
}

fn client_b() -> AiMappedProject {
    project("102", "Client B")
}

/// June 2026, closed as close 7.
fn june() -> AiAdjustedMonth {
    AiAdjustedMonth {
        month: month("2026-06"),
        close_id: AiMonthCloseId::new(7),
    }
}

/// An entry of `subject`'s `kind` to `project` (or an Unassigned kind) in
/// USD, paid by `payer`.
#[allow(clippy::too_many_arguments)]
fn entry(
    subject: AiBillingSubject,
    kind: AiAdjustedKind,
    mapped: Option<AiMappedProject>,
    unassigned: Option<AiUnassignedKind>,
    payer: AiKeyPayer,
    provider: AiProvider,
    amount: Option<i64>,
    sek: AiEntrySek,
) -> AiBilledEntry {
    AiBilledEntry {
        key: AiAdjustmentKey {
            subject: AiKeySubject::of(&subject),
            project: AiKeyProject::of(mapped.as_ref(), unassigned),
            kind,
            currency: usd(),
            payer,
        },
        subject,
        provider,
        project: mapped,
        unassigned,
        credits: kind.is_metered().then(|| amount.unwrap_or(0)),
        // Invoiced at June's 10.50 when its SEK is known.
        rate: matches!(sek, AiEntrySek::Known(_))
            .then(|| AiExchangeRateValue::parse("10.5").unwrap()),
        amount,
        sek,
    }
}

/// Ada's API usage of Claude billed to `mapped`.
fn api(mapped: AiMappedProject, cents: i64, sek: i64) -> AiBilledEntry {
    entry(
        AiBillingSubject::User(ADA),
        AiAdjustedKind::Api,
        Some(mapped),
        None,
        AiKeyPayer::Provider(AiProvider::Claude),
        AiProvider::Claude,
        Some(cents),
        AiEntrySek::Known(sek),
    )
}

/// Ada's metered credits to `mapped`.
fn metered(mapped: AiMappedProject, cents: i64, sek: i64) -> AiBilledEntry {
    entry(
        AiBillingSubject::User(ADA),
        AiAdjustedKind::Metered(AiMeteredChargeKind::Project),
        Some(mapped),
        None,
        AiKeyPayer::Provider(AiProvider::Copilot),
        AiProvider::Copilot,
        Some(cents),
        AiEntrySek::Known(sek),
    )
}

fn lines(
    recompute: &[AiBilledEntry],
    snapshot: &[AiBilledEntry],
    frozen: &[AiBilledEntry],
) -> Vec<AiAdjustmentLine> {
    adjustment_lines(
        june(),
        recompute,
        snapshot,
        frozen,
        &[rate(june().month, "10.5")],
        &HashMap::new(),
    )
    .unwrap()
}

/// A line as `(project, amount, sek, sek only)`.
fn summary(line: &AiAdjustmentLine) -> (Option<String>, String, Option<String>, bool) {
    (
        line.project.as_ref().map(|project| project.name.clone()),
        line.amount.to_decimal(),
        line.converted.amount().map(AiFeeAmount::to_decimal),
        line.sek_only(),
    )
}

fn row(
    project: &str,
    amount: &str,
    sek: &str,
    sek_only: bool,
) -> (Option<String>, String, Option<String>, bool) {
    (
        Some(project.to_string()),
        amount.to_string(),
        Some(sek.to_string()),
        sek_only,
    )
}

#[test]
fn keys_survive_renames_and_spelling() {
    let renamed = |login: &str| AiBillingSubject::ProviderAccount {
        provider: AiProvider::Copilot,
        account_id: AiProviderAccountId::new(1001).unwrap(),
        login: AiProviderLogin::parse(login).unwrap(),
    };
    assert_eq!(
        AiKeySubject::of(&renamed("ada-lovelace")),
        AiKeySubject::of(&renamed("ada-renamed"))
    );
    assert_eq!(
        AiKeySubject::of(&renamed("ada-lovelace")).as_text(),
        "account:copilot:1001"
    );
    let repository = |name: &str| AiBillingSubject::Repository {
        provider: AiProvider::Copilot,
        name: name.to_string(),
    };
    assert_eq!(
        AiKeySubject::of(&repository("Widget-API")).as_text(),
        "repository:copilot:widget-api"
    );
    assert_eq!(
        AiKeySubject::of(&repository("Widget-API")),
        AiKeySubject::of(&repository("widget-api"))
    );
    for text in [
        "user:42",
        "account:copilot:1001",
        "unknown:copilot",
        "repository:copilot:example:odd",
    ] {
        assert_eq!(AiKeySubject::parse(text).unwrap().as_text(), text);
    }
    for text in [
        "user:x",
        "account:copilot:0",
        "repository:copilot:",
        "team:1",
    ] {
        assert_eq!(AiKeySubject::parse(text), None, "{text}");
    }
    // The stored label brings the display name back.
    assert_eq!(
        AiKeySubject::parse("repository:copilot:widget-api")
            .unwrap()
            .subject(Some("Widget-API")),
        Some(repository("Widget-API"))
    );

    for (project, text) in [
        (AiKeyProject::Project(ProjectId::new("101")), "project:101"),
        (
            AiKeyProject::Unassigned(AiUnassignedKind::Unmapped),
            "unassigned:unmapped",
        ),
        (
            AiKeyProject::Unassigned(AiUnassignedKind::UnmappedRepository),
            "unassigned:unmapped_repository",
        ),
        (
            AiKeyProject::Unassigned(AiUnassignedKind::Unattributed),
            "unassigned:unattributed",
        ),
        (AiKeyProject::None, "none"),
    ] {
        assert_eq!(project.as_text(), text);
        assert_eq!(AiKeyProject::parse(text), Some(project));
    }
    for kind in [
        "api",
        "subscription",
        "overhead",
        "metered_project",
        "metered_personal",
        "metered_untracked",
        "metered_unlinked_account",
        "metered_external_account",
        "metered_unknown_account",
        "metered_repository",
    ] {
        assert_eq!(AiAdjustedKind::parse(kind).unwrap().as_str(), kind);
    }
}

fn usage(cost_cents: i64) -> AiBillingUsage {
    AiBillingUsage {
        records: 1,
        priced_cost: AiUsdNanos::new(cost_cents * 10_000_000).unwrap(),
        ..AiBillingUsage::default()
    }
}

fn day(provider: AiProvider, sign_in: bool, project: AiMappedProject, cents: i64) -> AiDailyUsage {
    AiDailyUsage {
        billing_provider: None,
        user_id: ADA,
        provider,
        day: date!(2026 - 06 - 10),
        chatgpt_sign_in: sign_in,
        project: AiBillingProject::Mapped(project),
        usage: usage(cents),
    }
}

#[test]
fn a_subscriptions_codex_and_pi_shares_sum_under_one_key() {
    let subscription = AiSubscription {
        id: AiSubscriptionId::new(5),
        user_id: ADA,
        terms: AiSubscriptionTerms {
            provider: AiProvider::Codex,
            plan: AiSubscriptionPlan::parse("ChatGPT Pro").unwrap(),
            monthly_cost: AiMonthlyCost::parse("200").unwrap(),
            currency: usd(),
            period: AiSubscriptionPeriod::new(date!(2026 - 01 - 01), None).unwrap(),
        },
    };
    let bill = bill_month(
        month("2026-06"),
        &[
            day(AiProvider::Codex, false, client_a(), 300),
            day(AiProvider::Pi, true, client_a(), 100),
            day(AiProvider::Codex, false, client_b(), 400),
        ],
        &[subscription],
    )
    .unwrap();
    let converted = convert_bill(&bill, &[rate(month("2026-06"), "10.5")], &[]).unwrap();
    let providers: HashMap<AiSubscriptionId, AiProvider> =
        HashMap::from([(AiSubscriptionId::new(5), AiProvider::Codex)]);
    let entries: Vec<AiBilledEntry> = bill
        .lines
        .iter()
        .zip(&converted.lines)
        .flat_map(|(line, converted)| line.parts(converted).unwrap())
        .map(|part| AiBilledEntry::of_part(&part, |id| providers.get(&id).copied()).unwrap())
        .collect();
    assert_eq!(entries.len(), 3);
    // Codex and Pi on Client A share a key, named after the subscription.
    let on = |name: &str| -> Vec<&AiBilledEntry> {
        entries
            .iter()
            .filter(|entry| {
                entry
                    .project
                    .as_ref()
                    .is_some_and(|project| project.name == name)
            })
            .collect()
    };
    let (on_a, on_b) = (on("Client A"), on("Client B"));
    assert_eq!(on_a.len(), 2);
    assert_eq!(on_a[0].key, on_a[1].key);
    assert_ne!(on_a[0].key, on_b[0].key);
    assert!(entries
        .iter()
        .all(|entry| entry.provider == AiProvider::Codex));
    assert_eq!(
        on_a[0].key.payer,
        AiKeyPayer::Subscription(AiSubscriptionId::new(5))
    );
    assert_eq!(
        on_a[0].key.columns(),
        (
            "user:1".to_string(),
            "project:101".to_string(),
            "subscription",
            "USD"
        )
    );

    // Snapshot as it is, then moving the Pi share's usage to Client B posts
    // one line per key: the sum of both of Client A's shares.
    let plans = HashMap::from([(AiSubscriptionId::new(5), "ChatGPT Pro".to_string())]);
    let unchanged =
        adjustment_lines(june(), &entries, &entries, &[], &converted.rates, &plans).unwrap();
    assert!(unchanged.is_empty());
    let moved = bill_month(
        month("2026-06"),
        &[
            day(AiProvider::Codex, false, client_a(), 300),
            day(AiProvider::Pi, true, client_b(), 100),
            day(AiProvider::Codex, false, client_b(), 400),
        ],
        &[AiSubscription {
            id: AiSubscriptionId::new(5),
            user_id: ADA,
            terms: AiSubscriptionTerms {
                provider: AiProvider::Codex,
                plan: AiSubscriptionPlan::parse("ChatGPT Pro").unwrap(),
                monthly_cost: AiMonthlyCost::parse("200").unwrap(),
                currency: usd(),
                period: AiSubscriptionPeriod::new(date!(2026 - 01 - 01), None).unwrap(),
            },
        }],
    )
    .unwrap();
    let moved_converted = convert_bill(&moved, &converted.rates, &[]).unwrap();
    let recompute: Vec<AiBilledEntry> = moved
        .lines
        .iter()
        .zip(&moved_converted.lines)
        .flat_map(|(line, converted)| line.parts(converted).unwrap())
        .map(|part| AiBilledEntry::of_part(&part, |id| providers.get(&id).copied()).unwrap())
        .collect();
    let adjusted =
        adjustment_lines(june(), &recompute, &entries, &[], &converted.rates, &plans).unwrap();
    // 200 USD split 3:1:4 → 75 + 25 on A; then 3 on A, 1 + 4 on B → 75 / 125.
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [
            row("Client A", "-25.00", "-262.50", false),
            row("Client B", "25.00", "262.50", false),
        ]
    );
    assert!(adjusted
        .iter()
        .all(|line| line.provider == AiProvider::Codex));
    assert!(adjusted
        .iter()
        .all(|line| line.plan.as_deref() == Some("ChatGPT Pro")));
}

#[test]
fn no_change_posts_nothing() {
    let entries = [api(client_a(), 1_250, 13_125), metered(client_b(), 70, 735)];
    assert!(lines(&entries, &entries, &[]).is_empty());
}

#[test]
fn a_mapping_change_moves_amount_and_sek_between_projects() {
    let snapshot = [api(client_a(), 1_250, 13_125)];
    let recompute = [api(client_b(), 1_250, 13_125)];
    let adjusted = lines(&recompute, &snapshot, &[]);
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [
            row("Client A", "-12.50", "-131.25", false),
            row("Client B", "12.50", "131.25", false),
        ]
    );
    let line = &adjusted[0];
    assert_eq!(line.adjusts, june());
    assert_eq!(line.credits, None);
    assert_eq!(
        line.converted
            .rate()
            .and_then(AiExchangeRate::rate)
            .map(AiExchangeRateValue::to_decimal),
        Some("10.50".to_string())
    );
}

#[test]
fn frozen_adjustments_net_out() {
    let snapshot = [metered(client_a(), 12, 126)];
    let recompute = [metered(client_b(), 12, 126)];
    // Already frozen in a later close: nothing more posts.
    let frozen = lines(&recompute, &snapshot, &[])
        .iter()
        .map(|line| AiBilledEntry {
            key: line.key.clone(),
            subject: line.subject.clone(),
            provider: line.provider,
            project: line.project.clone(),
            unassigned: line.unassigned,
            amount: Some(line.amount.hundredths),
            sek: AiEntrySek::of(&line.converted),
            rate: line.converted.rate().and_then(AiExchangeRate::rate),
            credits: line.credits,
        })
        .collect::<Vec<_>>();
    assert_eq!(frozen.len(), 2);
    assert!(lines(&recompute, &snapshot, &frozen).is_empty());

    // A second change, to Client C, posts only its own delta.
    let client_c = project("103", "Client C");
    let again = [metered(client_c, 12, 126)];
    let adjusted = lines(&again, &snapshot, &frozen);
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [
            row("Client B", "-0.12", "-1.26", false),
            row("Client C", "0.12", "1.26", false),
        ]
    );
    assert_eq!(adjusted[0].credits, Some(-12));
    assert_eq!(adjusted[1].credits, Some(12));
}

#[test]
fn an_owner_change_sums_two_accounts_under_one_key() {
    // One user, two accounts in the month (an owner change), one project:
    // two lines, one key. The account is not part of the key.
    let snapshot = [metered(client_a(), 40, 420), metered(client_a(), 30, 315)];
    let recompute = [metered(client_a(), 70, 735)];
    assert!(lines(&recompute, &snapshot, &[]).is_empty());
    let more = [metered(client_a(), 50, 525), metered(client_a(), 30, 315)];
    assert_eq!(
        lines(&more, &snapshot, &[])
            .iter()
            .map(summary)
            .collect::<Vec<_>>(),
        [row("Client A", "0.10", "1.05", false)]
    );
}

/// June's USD rate, fetched at 10.50, overridden to `raw` after the close.
fn overridden(raw: &str) -> AiExchangeRate {
    let mut overridden = rate(june().month, "10.5");
    overridden.overridden = Some(crate::domain::models::AiExchangeRateOverride {
        rate: AiExchangeRateValue::parse(raw).unwrap(),
        set_on: date!(2026 - 07 - 15),
        by: None,
    });
    overridden
}

#[test]
fn a_rate_override_posts_an_sek_only_adjustment() {
    let snapshot = [api(client_a(), 1_000, 10_500)];
    // June's rate overridden from 10.50 to 11.00 after the close.
    let recompute = [api(client_a(), 1_000, 11_000)];
    let adjusted = adjustment_lines(
        june(),
        &recompute,
        &snapshot,
        &[],
        &[overridden("11")],
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [row("Client A", "0.00", "5.00", true)]
    );
    assert!(adjusted[0].rate_changed);
    assert_eq!(
        adjusted[0]
            .converted
            .rate()
            .and_then(AiExchangeRate::rate)
            .map(AiExchangeRateValue::to_decimal),
        Some("11.00".to_string())
    );
}

#[test]
fn sek_only_rounding_at_an_unchanged_rate_names_no_rate_change() {
    // June's 10.00 USD split over two of Ada's lines to Client A when it
    // closed; recomputed, the key's SEK rounds one öre higher at the same
    // 10.50, as a fee's SEK split does when other projects' weights move.
    let snapshot = [api(client_a(), 500, 5_250), api(client_a(), 500, 5_250)];
    let recompute = [api(client_a(), 1_000, 10_501)];
    let adjusted = lines(&recompute, &snapshot, &[]);
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [row("Client A", "0.00", "0.01", true)]
    );
    assert!(!adjusted[0].rate_changed);
}

#[test]
fn rate_changed_compares_with_the_rate_last_invoiced() {
    let snapshot = [api(client_a(), 1_000, 10_500)];
    // An override to 11.00 posted 5.00 SEK, frozen in July's close.
    let mut frozen = api(client_a(), 0, 500);
    frozen.rate = AiExchangeRateValue::parse("11");
    let frozen = [frozen];
    let at = |raw: &str, recompute: AiBilledEntry| {
        adjustment_lines(
            june(),
            &[recompute],
            &snapshot,
            &frozen,
            &[overridden(raw)],
            &HashMap::new(),
        )
        .unwrap()
    };

    // Still at 11.00, the key's SEK rounds one öre higher: no rate change,
    // though the snapshot's rate differs.
    let rounding = at("11", api(client_a(), 1_000, 11_001));
    assert_eq!(
        rounding.iter().map(summary).collect::<Vec<_>>(),
        [row("Client A", "0.00", "0.01", true)]
    );
    assert!(!rounding[0].rate_changed);

    // Back at 10.50: the snapshot's rate again, but not the one last
    // invoiced.
    let back = at("10.5", api(client_a(), 1_000, 10_500));
    assert_eq!(
        back.iter().map(summary).collect::<Vec<_>>(),
        [row("Client A", "0.00", "-5.00", true)]
    );
    assert!(back[0].rate_changed);
}

#[test]
fn a_missing_rate_leaves_the_sek_difference_unknown() {
    let snapshot = [api(client_a(), 1_000, 10_500)];
    let mut recompute = api(client_a(), 1_200, 0);
    recompute.sek = AiEntrySek::MissingRate;
    let adjusted =
        adjustment_lines(june(), &[recompute], &snapshot, &[], &[], &HashMap::new()).unwrap();
    assert_eq!(adjusted.len(), 1);
    assert_eq!(adjusted[0].amount.to_decimal(), "2.00");
    assert_eq!(
        adjusted[0].converted,
        AiConvertedAmount::MissingRate { currency: usd() }
    );
    assert!(!adjusted[0].sek_only());
}

#[test]
fn snapshot_without_a_rate_counts_as_nothing_invoiced_in_sek() {
    // June closed without a USD rate (waived): nothing was invoiced in SEK.
    let mut snapshot = api(client_a(), 1_000, 0);
    snapshot.sek = AiEntrySek::Unknown;
    snapshot.rate = None;
    // Now the rate is there: the SEK amount posts, SEK only, and the rate is
    // what changed.
    let recompute = [api(client_a(), 1_000, 10_500)];
    let adjusted = lines(&recompute, &[snapshot], &[]);
    assert_eq!(
        adjusted.iter().map(summary).collect::<Vec<_>>(),
        [row("Client A", "0.00", "105.00", true)]
    );
    assert!(adjusted[0].rate_changed);
}

#[test]
fn an_unknown_amount_counts_as_zero() {
    let mut unpriced = api(client_a(), 0, 0);
    unpriced.amount = None;
    unpriced.sek = AiEntrySek::Unknown;
    let priced = [api(client_a(), 300, 3_150)];
    assert_eq!(
        lines(&priced, &[unpriced.clone()], &[])
            .iter()
            .map(summary)
            .collect::<Vec<_>>(),
        [row("Client A", "3.00", "31.50", false)]
    );
    assert!(lines(&[unpriced.clone()], &[unpriced], &[]).is_empty());
}

#[test]
fn a_vanished_line_is_credited_with_its_snapshot_names() {
    let account = AiBillingSubject::ProviderAccount {
        provider: AiProvider::Copilot,
        account_id: AiProviderAccountId::new(1001).unwrap(),
        login: AiProviderLogin::parse("ada-lovelace").unwrap(),
    };
    let unlinked = entry(
        account.clone(),
        AiAdjustedKind::Metered(AiMeteredChargeKind::Account(
            AiMeteredAccountReason::Unlinked,
        )),
        None,
        None,
        AiKeyPayer::Provider(AiProvider::Copilot),
        AiProvider::Copilot,
        Some(500),
        AiEntrySek::Known(5_250),
    );
    // Linking the account moved its credits to Ada.
    let linked = metered(client_a(), 500, 5_250);
    let adjusted = lines(&[linked], &[unlinked], &[]);
    assert_eq!(adjusted.len(), 2);
    let credit = adjusted
        .iter()
        .find(|line| line.subject == account)
        .unwrap();
    assert_eq!(credit.amount.to_decimal(), "-5.00");
    assert_eq!(credit.credits, Some(-500));
    assert_eq!(credit.converted.amount().unwrap().to_decimal(), "-52.50");
    // Ada's project line comes first, Spinit's account line last.
    assert_eq!(adjusted[1].subject, account);
}

#[test]
fn overhead_turned_into_shares_posts_both_sides() {
    let subscription = AiSubscriptionId::new(9);
    let in_sek = |mut entry: AiBilledEntry| {
        entry.key.currency = AiCurrency::billing();
        entry
    };
    let fee = |kind: AiAdjustedKind, mapped: Option<AiMappedProject>, amount: i64| {
        in_sek(entry(
            AiBillingSubject::User(GRACE),
            kind,
            mapped,
            None,
            AiKeyPayer::Subscription(subscription),
            AiProvider::Claude,
            Some(amount),
            AiEntrySek::Known(amount),
        ))
    };
    let overhead = fee(AiAdjustedKind::Overhead, None, 110_000);
    let share = |mapped: AiMappedProject, amount: i64| {
        fee(AiAdjustedKind::Subscription, Some(mapped), amount)
    };
    // Usage uploaded late: the overhead became two shares.
    let recompute = [share(client_a(), 66_000), share(client_b(), 44_000)];
    let adjusted = adjustment_lines(
        june(),
        &recompute,
        &[overhead],
        &[],
        &[],
        &HashMap::from([(subscription, "Claude Max 5x".to_string())]),
    )
    .unwrap();
    let summarized: Vec<(Option<String>, &str, String, Option<String>)> = adjusted
        .iter()
        .map(|line| {
            (
                line.project.as_ref().map(|project| project.name.clone()),
                line.key.kind.as_str(),
                line.amount.to_decimal(),
                line.converted.amount().map(AiFeeAmount::to_decimal),
            )
        })
        .collect();
    assert_eq!(
        summarized,
        [
            (
                Some("Client A".to_string()),
                "subscription",
                "660.00".to_string(),
                Some("660.00".to_string())
            ),
            (
                Some("Client B".to_string()),
                "subscription",
                "440.00".to_string(),
                Some("440.00".to_string())
            ),
            (
                None,
                "overhead",
                "-1100.00".to_string(),
                Some("-1100.00".to_string())
            ),
        ]
    );
    // SEK lines carry no rate.
    assert!(adjusted.iter().all(|line| line.converted.rate().is_none()));
    assert!(adjusted
        .iter()
        .all(|line| line.plan.as_deref() == Some("Claude Max 5x")));
}

fn close(id: i64, raw: &str) -> AiMonthClose {
    AiMonthClose {
        id: AiMonthCloseId::new(id),
        month: month(raw),
        closed_at: datetime!(2026-10-01 08:00 UTC),
        closed_by: None,
        reopened_at: None,
        reopened_by: None,
    }
}

#[test]
fn adjustments_post_on_the_first_open_month_after_each_close() {
    // June and July closed, August open, September closed, October open.
    let active = [
        close(1, "2026-06"),
        close(2, "2026-07"),
        close(3, "2026-09"),
    ];
    assert_eq!(next_open(month("2026-06"), &active), month("2026-08"));
    assert_eq!(next_open(month("2026-07"), &active), month("2026-08"));
    assert_eq!(next_open(month("2026-09"), &active), month("2026-10"));
    assert_eq!(
        posting_on(month("2026-08"), &active),
        [
            AiAdjustedMonth {
                month: month("2026-06"),
                close_id: AiMonthCloseId::new(1),
            },
            AiAdjustedMonth {
                month: month("2026-07"),
                close_id: AiMonthCloseId::new(2),
            },
        ]
    );
    assert_eq!(
        posting_on(month("2026-10"), &active),
        [AiAdjustedMonth {
            month: month("2026-09"),
            close_id: AiMonthCloseId::new(3),
        }]
    );
    assert!(posting_on(month("2026-05"), &active).is_empty());
    // A re-opened close is history: it neither holds nor skips a month.
    let reopened = AiMonthClose {
        reopened_at: Some(datetime!(2026-10-02 08:00 UTC)),
        ..close(4, "2026-08")
    };
    let with_history = [active.as_slice(), &[reopened]].concat();
    assert_eq!(next_open(month("2026-07"), &with_history), month("2026-08"));
    assert_eq!(following(month("2026-12")), month("2027-01"));
}

#[test]
fn a_month_closes_once_it_is_over_in_utc_and_the_billing_zone() {
    let august = month("2026-08");
    let over_at = datetime!(2026-09-01 00:00 UTC);
    let in_progress = Some(AiCloseBlocker::MonthInProgress { over_at });
    // The last day in UTC, or still in the zone: in progress.
    assert_eq!(
        close_blocker(
            august,
            &[],
            date!(2026 - 08 - 31),
            date!(2026 - 09 - 01),
            over_at
        ),
        in_progress
    );
    assert_eq!(
        close_blocker(
            august,
            &[],
            date!(2026 - 09 - 01),
            date!(2026 - 08 - 31),
            over_at
        ),
        in_progress
    );
    assert_eq!(
        close_blocker(
            august,
            &[],
            date!(2026 - 09 - 01),
            date!(2026 - 09 - 01),
            over_at
        ),
        None
    );
    assert_eq!(
        close_blocker(
            august,
            &[close(1, "2026-08")],
            date!(2026 - 10 - 01),
            date!(2026 - 10 - 01),
            over_at
        ),
        Some(AiCloseBlocker::AlreadyClosed)
    );
}

/// Invoiced for June: its snapshot, the adjustments to it frozen in active
/// closes, and the live adjustment on its next open month; per key, in
/// amount and SEK.
fn invoiced(
    snapshot: &[AiBilledEntry],
    frozen: &[AiBilledEntry],
    live: &[AiAdjustmentLine],
) -> BTreeMap<String, (i64, i64)> {
    let mut sums: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for entry in snapshot.iter().chain(frozen) {
        let sum = sums.entry(entry.key.project.as_text()).or_default();
        sum.0 += entry.amount.unwrap_or(0);
        sum.1 += entry.sek.stored().unwrap_or(0);
    }
    for line in live {
        let sum = sums.entry(line.key.project.as_text()).or_default();
        sum.0 += line.amount.hundredths;
        sum.1 += line
            .converted
            .amount()
            .map_or(0, |amount| amount.hundredths);
    }
    sums.retain(|_, sum| *sum != (0, 0));
    sums
}

fn as_entries(lines: &[AiAdjustmentLine]) -> Vec<AiBilledEntry> {
    lines
        .iter()
        .map(|line| AiBilledEntry {
            key: line.key.clone(),
            subject: line.subject.clone(),
            provider: line.provider,
            project: line.project.clone(),
            unassigned: line.unassigned,
            amount: Some(line.amount.hundredths),
            sek: AiEntrySek::of(&line.converted),
            rate: line.converted.rate().and_then(AiExchangeRate::rate),
            credits: line.credits,
        })
        .collect()
}

#[test]
fn reopening_a_middle_month_keeps_every_correction_billed_once() {
    // June closed (snapshot S on Client A). July closed holding nothing.
    // A remap to Client B posts on August, which closes and freezes A₁.
    // Then July is re-opened: A₁ stays frozen in August's (later) close,
    // and June's adjustments post on July again.
    let snapshot = [api(client_a(), 1_000, 10_500)];
    let recompute_b = [api(client_b(), 1_000, 10_500)];
    let first = lines(&recompute_b, &snapshot, &[]);
    let frozen_in_august = as_entries(&first);
    let truth = |recompute: &[AiBilledEntry]| invoiced(recompute, &[], &[]);

    // July re-opened: nothing new to post, whichever close holds A₁.
    let live = lines(&recompute_b, &snapshot, &frozen_in_august);
    assert!(live.is_empty());
    assert_eq!(
        invoiced(&snapshot, &frozen_in_august, &live),
        truth(&recompute_b)
    );

    // A second remap, to Client C, while July is open: only its delta.
    let recompute_c = [api(project("103", "Client C"), 1_000, 10_500)];
    let live = lines(&recompute_c, &snapshot, &frozen_in_august);
    assert_eq!(live.len(), 2);
    assert_eq!(
        invoiced(&snapshot, &frozen_in_august, &live),
        truth(&recompute_c)
    );

    // Re-opening August instead puts A₁ back live: still exact.
    let live = lines(&recompute_c, &snapshot, &[]);
    assert_eq!(invoiced(&snapshot, &[], &live), truth(&recompute_c));
}

#[test]
fn metered_lines_parts_carry_their_credits() {
    let line = AiBillingLine {
        subject: AiBillingSubject::Repository {
            provider: AiProvider::Copilot,
            name: "Widget-API".to_string(),
        },
        provider: AiProvider::Copilot,
        project: None,
        charge: AiBillingCharge::Metered(AiMeteredCharge {
            kind: AiMeteredChargeKind::Repository,
            account: None,
            sku: Some("Copilot AI Credits".to_string()),
            amount: AiFeeAmount {
                hundredths: 8,
                currency: usd(),
            },
            allocated: AiUsdNanos::new(80_000_000).unwrap(),
            provisional: Vec::new(),
            low_ratio: false,
        }),
        usage: AiBillingUsage::default(),
        unassigned: vec![crate::domain::models::AiUnassignedUsage {
            kind: AiUnassignedKind::UnmappedRepository,
            usage: AiBillingUsage::default(),
        }],
    };
    let converted = AiConvertedAmount::Converted {
        amount: AiFeeAmount {
            hundredths: 84,
            currency: AiCurrency::billing(),
        },
        rate: Some(rate(june().month, "10.5")),
    };
    let parts = line.parts(&converted).unwrap();
    let entry = AiBilledEntry::of_part(&parts[0], |_| None).unwrap();
    assert_eq!(
        entry.key.columns(),
        (
            "repository:copilot:widget-api".to_string(),
            "unassigned:unmapped_repository".to_string(),
            "metered_repository",
            "USD"
        )
    );
    assert_eq!(entry.credits, Some(8));
    assert_eq!(entry.sek, AiEntrySek::Known(84));
}
