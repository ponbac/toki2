use time::macros::date;

use super::*;
use crate::domain::models::{
    AiMonthlyCost, AiSubscriptionPeriod, AiSubscriptionPlan, AiSubscriptionTerms, ProjectId,
};

const DEV: UserId = UserId::new(7);
const OTHER_DEV: UserId = UserId::new(8);

fn september() -> AiBillingMonth {
    AiBillingMonth::parse("2026-09").unwrap()
}

fn project(id: &str, name: &str) -> Option<AiMappedProject> {
    Some(AiMappedProject {
        id: ProjectId::new(id),
        name: name.to_string(),
    })
}

fn client_a() -> Option<AiMappedProject> {
    project("101", "Client A")
}

fn client_b() -> Option<AiMappedProject> {
    project("202", "Client B")
}

/// Usage priced at `cents` US cents, with `tokens` input tokens.
fn priced(cents: i64, tokens: i64) -> AiBillingUsage {
    AiBillingUsage {
        tokens: AiTokenCounts {
            input: tokens,
            ..AiTokenCounts::default()
        },
        records: 1,
        priced_cost: AiUsdNanos::new(cents * 10_000_000).unwrap(),
        unpriced_records: 0,
    }
}

/// Usage with `records` unpriced records and `tokens` input tokens.
fn unpriced(records: i64, tokens: i64) -> AiBillingUsage {
    AiBillingUsage {
        tokens: AiTokenCounts {
            input: tokens,
            ..AiTokenCounts::default()
        },
        records,
        priced_cost: AiUsdNanos::ZERO,
        unpriced_records: records,
    }
}

fn day_usage(
    user_id: UserId,
    provider: AiProvider,
    day: Date,
    project: Option<AiMappedProject>,
    usage: AiBillingUsage,
) -> AiDailyUsage {
    AiDailyUsage {
        user_id,
        provider,
        day,
        project,
        usage,
    }
}

fn claude(day: Date, project: Option<AiMappedProject>, usage: AiBillingUsage) -> AiDailyUsage {
    day_usage(DEV, AiProvider::Claude, day, project, usage)
}

fn subscription(
    id: i32,
    user_id: UserId,
    provider: AiProvider,
    fee: &str,
    currency: &str,
    valid_from: Date,
    valid_to: Option<Date>,
) -> AiSubscription {
    AiSubscription {
        id: AiSubscriptionId::new(id),
        user_id,
        terms: AiSubscriptionTerms {
            provider,
            plan: AiSubscriptionPlan::parse("Claude Max 5x").unwrap(),
            monthly_cost: AiMonthlyCost::parse(fee).unwrap(),
            currency: AiCurrency::parse(currency).unwrap(),
            period: AiSubscriptionPeriod::new(valid_from, valid_to).unwrap(),
        },
    }
}

fn claude_max(id: i32, valid_from: Date, valid_to: Option<Date>) -> AiSubscription {
    subscription(
        id,
        DEV,
        AiProvider::Claude,
        "1100",
        "SEK",
        valid_from,
        valid_to,
    )
}

/// A line as `(project name or "Unassigned"/"Overhead", charge, fee, usage cents)`.
type Summary = (String, &'static str, Option<String>, i64);

fn summary(line: &AiBillingLine) -> Summary {
    let (charge, fee) = match &line.charge {
        AiBillingCharge::Api => ("api", None),
        AiBillingCharge::Subscription { fee, .. } => ("subscription", Some(fee)),
        AiBillingCharge::UnallocatedOverhead { fee, .. } => ("overhead", Some(fee)),
    };
    let project = match (&line.project, &line.charge) {
        (_, AiBillingCharge::UnallocatedOverhead { .. }) => "Overhead".to_string(),
        (Some(project), _) => project.name.clone(),
        (None, _) => "Unassigned".to_string(),
    };

    (
        project,
        charge,
        fee.map(|fee| format!("{} {}", fee.to_decimal(), fee.currency.as_str())),
        line.usage.priced_cost.nanos() / 10_000_000,
    )
}

fn summaries(bill: &AiMonthBill) -> Vec<Summary> {
    bill.lines.iter().map(summary).collect()
}

fn line(project: &str, charge: &'static str, fee: Option<&str>, cents: i64) -> Summary {
    (project.to_string(), charge, fee.map(str::to_string), cents)
}

fn fee_sum(bill: &AiMonthBill, subscription_id: i32) -> i64 {
    bill.lines
        .iter()
        .filter_map(|line| match &line.charge {
            AiBillingCharge::Subscription {
                subscription_id: id,
                fee,
            }
            | AiBillingCharge::UnallocatedOverhead {
                subscription_id: id,
                fee,
            } if id.as_i32() == subscription_id => Some(fee.hundredths),
            _ => None,
        })
        .sum()
}

#[test]
fn months_parse_as_year_and_month_and_know_their_days() {
    let month = september();
    assert_eq!(month.to_string(), "2026-09");
    assert_eq!(
        (month.first_day(), month.last_day(), month.dates().end()),
        (
            date!(2026 - 09 - 01),
            date!(2026 - 09 - 30),
            date!(2026 - 10 - 01)
        )
    );
    assert_eq!(month.days(), 30);
    assert_eq!(AiBillingMonth::parse("2028-02").unwrap().days(), 29);
    assert_eq!(AiBillingMonth::parse("2027-02").unwrap().days(), 28);
    let december = AiBillingMonth::parse("2026-12").unwrap();
    assert_eq!(december.dates().end(), date!(2027 - 01 - 01));
    assert!(september().contains(date!(2026 - 09 - 30)));
    assert!(!september().contains(date!(2026 - 10 - 01)));

    for invalid in [
        "",
        "2026-9",
        "2026-13",
        "2026-00",
        "26-09",
        "2026/09",
        "2026-09-01",
        "+026-09",
        "9999-12",
        "0000-06",
    ] {
        assert_eq!(AiBillingMonth::parse(invalid), None, "{invalid:?}");
    }
}

#[test]
fn fees_pro_rate_by_covered_days_rounded_half_up() {
    // A whole month bills exactly the fee.
    assert_eq!(prorate_fee(110_000, 30, 30).unwrap(), 110_000);
    assert_eq!(prorate_fee(99_999, 31, 31).unwrap(), 99_999);
    assert_eq!(prorate_fee(110_000, 15, 30).unwrap(), 55_000);
    // 199.00 × 10 / 31 = 64.1935…
    assert_eq!(prorate_fee(19_900, 10, 31).unwrap(), 6_419);
    // 0.01 × 1 / 2 = 0.005 rounds up; 0.03 × 1 / 4 = 0.0075 rounds up too.
    assert_eq!(prorate_fee(1, 1, 2).unwrap(), 1);
    assert_eq!(prorate_fee(3, 1, 4).unwrap(), 1);
    // 0.01 × 1 / 3 = 0.0033… rounds down.
    assert_eq!(prorate_fee(1, 1, 3).unwrap(), 0);
    assert_eq!(prorate_fee(0, 12, 30).unwrap(), 0);
    // The largest fee a subscription can store does not overflow.
    assert_eq!(
        prorate_fee(999_999_999_999, 31, 31).unwrap(),
        999_999_999_999
    );
}

#[test]
fn largest_remainder_sums_exactly_and_breaks_ties_to_the_earlier_part() {
    assert_eq!(
        allocate_largest_remainder(10_000, &[1, 1, 1]).unwrap(),
        Some(vec![3_334, 3_333, 3_333])
    );
    // 1100.00 by 3 : 1 : 0.5 is 733.33…, 244.44… and 122.22…; the left-over
    // hundredth goes to the largest remainder, 0.44.
    assert_eq!(
        allocate_largest_remainder(110_000, &[3_000, 1_000, 500]).unwrap(),
        Some(vec![73_333, 24_445, 12_222])
    );
    assert_eq!(
        allocate_largest_remainder(100, &[0, 7]).unwrap(),
        Some(vec![0, 100])
    );
    assert_eq!(
        allocate_largest_remainder(0, &[2, 5]).unwrap(),
        Some(vec![0, 0])
    );
    assert_eq!(allocate_largest_remainder(100, &[0, 0]).unwrap(), None);
    assert_eq!(allocate_largest_remainder(100, &[]).unwrap(), None);
    assert_eq!(allocate_largest_remainder(-1, &[1]).unwrap(), None);

    // Awkward weights and totals always sum exactly, and no part is off
    // its exact share by a hundredth or more.
    let mut seed: u64 = 0x5eed;
    let mut next = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        seed >> 33
    };
    for _ in 0..500 {
        let total = i64::try_from(next() % 10_000_000).unwrap();
        let weights: Vec<u128> = (0..1 + next() % 9)
            .map(|_| u128::from(next() % 1_000_000_000_000))
            .collect();
        let weight_sum: u128 = weights.iter().sum();
        let Some(parts) = allocate_largest_remainder(total, &weights).unwrap() else {
            assert_eq!(weight_sum, 0);
            continue;
        };
        assert_eq!(parts.iter().sum::<i64>(), total, "{total} by {weights:?}");
        for (part, weight) in parts.iter().zip(&weights) {
            let exact = total as f64 * *weight as f64 / weight_sum as f64;
            assert!((*part as f64 - exact).abs() < 1.0, "{part} vs {exact}");
        }
    }
}

#[test]
fn api_days_bill_each_projects_estimated_cost() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 02), client_b(), priced(250, 10)),
            claude(date!(2026 - 09 - 03), client_a(), priced(100, 10)),
            claude(date!(2026 - 09 - 04), client_a(), priced(40, 10)),
            claude(date!(2026 - 09 - 04), None, priced(7, 10)),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "api", None, 140),
            line("Client B", "api", None, 250),
            line("Unassigned", "api", None, 7),
        ]
    );
    assert!(bill.subscriptions.is_empty());
}

#[test]
fn a_whole_month_subscription_splits_its_fee_by_api_equivalent_cost() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 01), client_a(), priced(300, 10)),
            claude(date!(2026 - 09 - 15), client_b(), priced(100, 10)),
            claude(date!(2026 - 09 - 30), None, priced(50, 10)),
        ],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("733.33 SEK"), 300),
            line("Client B", "subscription", Some("244.45 SEK"), 100),
            line("Unassigned", "subscription", Some("122.22 SEK"), 50),
        ]
    );
    let [month] = bill.subscriptions.as_slice() else {
        panic!("expected one subscription month");
    };
    assert_eq!(month.covered_days(), 30);
    assert_eq!(month.prorated_fee.to_decimal(), "1100.00");
    assert_eq!(month.basis, Some(AiAllocationBasis::ApiCost));
    assert_eq!(month.usage.priced_cost.nanos(), 4_500_000_000);
    assert_eq!(fee_sum(&bill, 1), 110_000);
}

#[test]
fn a_subscription_starting_mid_month_pro_rates_and_leaves_earlier_days_to_api() {
    // Covers 16–30 September: 15 of 30 days.
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 15), client_a(), priced(80, 10)),
            claude(date!(2026 - 09 - 16), client_a(), priced(10, 10)),
            claude(date!(2026 - 09 - 29), client_b(), priced(30, 10)),
        ],
        &[claude_max(1, date!(2026 - 09 - 16), None)],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("137.50 SEK"), 10),
            line("Client B", "subscription", Some("412.50 SEK"), 30),
            line("Client A", "api", None, 80),
        ]
    );
    assert_eq!(bill.subscriptions[0].covered_days(), 15);
    assert_eq!(bill.subscriptions[0].prorated_fee.to_decimal(), "550.00");
    assert_eq!(fee_sum(&bill, 1), 55_000);
}

#[test]
fn a_subscription_ending_mid_month_pro_rates_and_leaves_later_days_to_api() {
    // Covers 1–10 September; 199.00 × 10 / 30 = 66.333… rounds to 66.33.
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 10), client_a(), priced(5, 10)),
            claude(date!(2026 - 09 - 11), client_a(), priced(60, 10)),
        ],
        &[subscription(
            1,
            DEV,
            AiProvider::Claude,
            "199",
            "USD",
            date!(2026 - 08 - 01),
            Some(date!(2026 - 09 - 10)),
        )],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("66.33 USD"), 5),
            line("Client A", "api", None, 60),
        ]
    );
    assert_eq!(bill.subscriptions[0].covered_days(), 10);
}

#[test]
fn a_mid_month_plan_change_bills_each_plan_for_its_own_days() {
    let pro = AiSubscription {
        terms: AiSubscriptionTerms {
            plan: AiSubscriptionPlan::parse("Claude Pro").unwrap(),
            monthly_cost: AiMonthlyCost::parse("220").unwrap(),
            ..claude_max(1, date!(2026 - 06 - 01), Some(date!(2026 - 09 - 14))).terms
        },
        ..claude_max(1, date!(2026 - 06 - 01), Some(date!(2026 - 09 - 14)))
    };
    let max = claude_max(2, date!(2026 - 09 - 15), None);
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 02), client_a(), priced(10, 10)),
            claude(date!(2026 - 09 - 14), client_b(), priced(30, 10)),
            claude(date!(2026 - 09 - 15), client_b(), priced(70, 10)),
        ],
        // Listed latest first, as the store orders them.
        &[max, pro],
    )
    .unwrap();

    // Pro: 220.00 × 14 / 30 = 102.666… → 102.67, split 1 : 3.
    // Max: 1100.00 × 16 / 30 = 586.666… → 586.67, all Client B.
    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("25.67 SEK"), 10),
            line("Client B", "subscription", Some("77.00 SEK"), 30),
            line("Client B", "subscription", Some("586.67 SEK"), 70),
        ]
    );
    let covered: Vec<(i32, i64, String)> = bill
        .subscriptions
        .iter()
        .map(|month| {
            (
                month.subscription.id.as_i32(),
                month.covered_days(),
                month.prorated_fee.to_decimal(),
            )
        })
        .collect();
    assert_eq!(
        covered,
        [(1, 14, "102.67".to_string()), (2, 16, "586.67".to_string())]
    );
    assert_eq!(fee_sum(&bill, 1), 10_267);
    assert_eq!(fee_sum(&bill, 2), 58_667);
}

#[test]
fn a_subscription_without_usage_is_unallocated_overhead() {
    // No usage at all in the month.
    let idle = bill_month(
        september(),
        &[],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();
    assert_eq!(
        summaries(&idle),
        [line("Overhead", "overhead", Some("1100.00 SEK"), 0)]
    );
    assert_eq!(idle.subscriptions[0].basis, None);
    assert_eq!(idle.lines[0].project, None);

    // Usage only on days the subscription does not cover is API usage; the
    // subscription's covered days have none, so its fee is overhead.
    let before_start = bill_month(
        september(),
        &[claude(date!(2026 - 09 - 05), client_a(), priced(40, 10))],
        &[claude_max(1, date!(2026 - 09 - 21), None)],
    )
    .unwrap();
    assert_eq!(
        summaries(&before_start),
        [
            line("Overhead", "overhead", Some("366.67 SEK"), 0),
            line("Client A", "api", None, 40),
        ]
    );

    // Another provider's usage does not use a Claude subscription.
    let other_provider = bill_month(
        september(),
        &[day_usage(
            DEV,
            AiProvider::Codex,
            date!(2026 - 09 - 05),
            client_a(),
            priced(40, 10),
        )],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();
    assert_eq!(
        other_provider
            .lines
            .iter()
            .map(|line| (line.provider, summary(line)))
            .collect::<Vec<_>>(),
        [
            (
                AiProvider::Claude,
                line("Overhead", "overhead", Some("1100.00 SEK"), 0)
            ),
            (AiProvider::Codex, line("Client A", "api", None, 40)),
        ]
    );
}

#[test]
fn a_subscription_with_some_used_days_is_not_overhead_on_its_idle_days() {
    let bill = bill_month(
        september(),
        &[claude(date!(2026 - 09 - 30), client_a(), priced(1, 1))],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [line("Client A", "subscription", Some("1100.00 SEK"), 1)]
    );
}

#[test]
fn partly_unpriced_usage_splits_by_the_priced_subtotal_and_stays_visible() {
    let mut mixed = priced(100, 10);
    mixed.add(&unpriced(3, 500)).unwrap();
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), mixed),
            claude(date!(2026 - 09 - 04), client_b(), unpriced(2, 10_000)),
            claude(date!(2026 - 09 - 05), None, priced(100, 10)),
        ],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();

    // Client B's usage is all unpriced, so it has no weight despite its
    // tokens; its unpriced records stay on its line.
    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("550.00 SEK"), 100),
            line("Client B", "subscription", Some("0.00 SEK"), 0),
            line("Unassigned", "subscription", Some("550.00 SEK"), 100),
        ]
    );
    let unpriced_records: Vec<i64> = bill
        .lines
        .iter()
        .map(|line| line.usage.unpriced_records)
        .collect();
    assert_eq!(unpriced_records, [3, 2, 0]);
    assert_eq!(
        bill.subscriptions[0].basis,
        Some(AiAllocationBasis::ApiCost)
    );
    assert_eq!(bill.subscriptions[0].usage.unpriced_records, 5);
}

#[test]
fn all_unpriced_usage_splits_by_token_share() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), unpriced(1, 3_000)),
            claude(date!(2026 - 09 - 04), client_b(), unpriced(4, 1_000)),
        ],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("825.00 SEK"), 0),
            line("Client B", "subscription", Some("275.00 SEK"), 0),
        ]
    );
    assert_eq!(bill.subscriptions[0].basis, Some(AiAllocationBasis::Tokens));
    assert_eq!(bill.subscriptions[0].usage.unpriced_records, 5);
}

#[test]
fn free_usage_splits_by_tokens_and_empty_usage_by_records() {
    let free = |tokens| AiBillingUsage {
        records: 1,
        ..priced(0, tokens)
    };
    let by_tokens = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), free(1)),
            claude(date!(2026 - 09 - 04), client_b(), free(3)),
        ],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();
    assert_eq!(
        by_tokens.subscriptions[0].basis,
        Some(AiAllocationBasis::Tokens)
    );
    assert_eq!(fee_sum(&by_tokens, 1), 110_000);

    let mut two_records = free(0);
    two_records.records = 2;
    let by_records = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), free(0)),
            claude(date!(2026 - 09 - 04), client_b(), two_records),
        ],
        &[claude_max(1, date!(2026 - 01 - 01), None)],
    )
    .unwrap();
    assert_eq!(
        by_records.subscriptions[0].basis,
        Some(AiAllocationBasis::Records)
    );
    assert_eq!(
        summaries(&by_records),
        [
            line("Client A", "subscription", Some("366.67 SEK"), 0),
            line("Client B", "subscription", Some("733.33 SEK"), 0),
        ]
    );
}

#[test]
fn equal_shares_that_do_not_divide_evenly_still_sum_to_the_fee() {
    let usage: Vec<AiDailyUsage> = [client_a(), client_b(), project("303", "Client C"), None]
        .into_iter()
        .map(|project| claude(date!(2026 - 09 - 10), project, priced(1, 1)))
        .collect();
    let bill = bill_month(
        september(),
        &usage,
        &[subscription(
            1,
            DEV,
            AiProvider::Claude,
            "0.03",
            "EUR",
            date!(2026 - 01 - 01),
            None,
        )],
    )
    .unwrap();

    assert_eq!(
        summaries(&bill),
        [
            line("Client A", "subscription", Some("0.01 EUR"), 1),
            line("Client B", "subscription", Some("0.01 EUR"), 1),
            line("Client C", "subscription", Some("0.01 EUR"), 1),
            line("Unassigned", "subscription", Some("0.00 EUR"), 1),
        ]
    );
}

#[test]
fn days_are_local_dates_so_daylight_saving_changes_do_not_alter_pro_rating() {
    // Stockholm moves to summer time on 29 March 2026 (a 23-hour day) and
    // back on 25 October (a 25-hour day). Pro-rating counts days.
    let march = AiBillingMonth::parse("2026-03").unwrap();
    let from_dst_day = bill_month(
        march,
        &[claude(date!(2026 - 03 - 29), client_a(), priced(10, 1))],
        &[claude_max(1, date!(2026 - 03 - 29), None)],
    )
    .unwrap();
    assert_eq!(from_dst_day.subscriptions[0].covered_days(), 3);
    // 1100.00 × 3 / 31 = 106.451…
    assert_eq!(
        summaries(&from_dst_day),
        [line("Client A", "subscription", Some("106.45 SEK"), 10)]
    );

    let october = AiBillingMonth::parse("2026-10").unwrap();
    let through_dst_day = bill_month(
        october,
        &[
            claude(date!(2026 - 10 - 25), client_a(), priced(10, 1)),
            claude(date!(2026 - 10 - 26), client_a(), priced(20, 1)),
        ],
        &[claude_max(
            1,
            date!(2026 - 09 - 01),
            Some(date!(2026 - 10 - 25)),
        )],
    )
    .unwrap();
    assert_eq!(through_dst_day.subscriptions[0].covered_days(), 25);
    assert_eq!(
        summaries(&through_dst_day),
        [
            line("Client A", "subscription", Some("887.10 SEK"), 10),
            line("Client A", "api", None, 20),
        ]
    );
}

#[test]
fn only_the_months_days_are_billed() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 08 - 31), client_a(), priced(1, 1)),
            claude(date!(2026 - 09 - 01), client_a(), priced(10, 1)),
            claude(date!(2026 - 09 - 30), client_a(), priced(20, 1)),
            claude(date!(2026 - 10 - 01), client_a(), priced(100, 1)),
        ],
        // Ends on the last day of August: none of September.
        &[claude_max(
            1,
            date!(2026 - 08 - 01),
            Some(date!(2026 - 08 - 31)),
        )],
    )
    .unwrap();

    assert_eq!(summaries(&bill), [line("Client A", "api", None, 30)]);
    assert!(bill.subscriptions.is_empty());
}

#[test]
fn subscriptions_apply_only_to_their_own_user_and_provider() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), priced(10, 1)),
            day_usage(
                OTHER_DEV,
                AiProvider::Claude,
                date!(2026 - 09 - 03),
                client_a(),
                priced(20, 1),
            ),
        ],
        &[subscription(
            1,
            OTHER_DEV,
            AiProvider::Codex,
            "200",
            "USD",
            date!(2026 - 01 - 01),
            None,
        )],
    )
    .unwrap();

    let lines: Vec<(i32, AiProvider, Summary)> = bill
        .lines
        .iter()
        .map(|line| (line.user_id.as_i32(), line.provider, summary(line)))
        .collect();
    assert_eq!(
        lines,
        [
            (7, AiProvider::Claude, line("Client A", "api", None, 10)),
            (8, AiProvider::Claude, line("Client A", "api", None, 20)),
            (
                8,
                AiProvider::Codex,
                line("Overhead", "overhead", Some("200.00 USD"), 0)
            ),
        ]
    );
}

#[test]
fn keys_mapped_to_one_project_bill_as_that_project() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), priced(10, 1)),
            claude(
                date!(2026 - 09 - 03),
                project("101", "Client A (renamed)"),
                priced(20, 1),
            ),
        ],
        &[],
    )
    .unwrap();

    assert_eq!(summaries(&bill), [line("Client A", "api", None, 30)]);
}

#[test]
fn totals_keep_currencies_apart_and_separate_overhead_and_unpriced_api_usage() {
    let mut unpriced_api = priced(40, 10);
    unpriced_api.add(&unpriced(2, 10)).unwrap();
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), priced(100, 10)),
            day_usage(
                DEV,
                AiProvider::Codex,
                date!(2026 - 09 - 03),
                None,
                unpriced_api,
            ),
        ],
        &[
            claude_max(1, date!(2026 - 01 - 01), None),
            subscription(
                2,
                OTHER_DEV,
                AiProvider::Codex,
                "200",
                "USD",
                date!(2026 - 01 - 01),
                None,
            ),
        ],
    )
    .unwrap();

    let totals = bill.totals().unwrap();
    let fees: Vec<(&str, String, String)> = totals
        .fees
        .iter()
        .map(|total| {
            (
                total.billed.currency.as_str(),
                total.billed.to_decimal(),
                total.overhead.to_decimal(),
            )
        })
        .collect();
    assert_eq!(
        fees,
        [
            ("SEK", "1100.00".to_string(), "0.00".to_string()),
            ("USD", "200.00".to_string(), "200.00".to_string())
        ]
    );
    assert_eq!(
        (
            totals.api_billed.to_decimal(),
            totals.api_billed.currency.as_str()
        ),
        ("0.40".to_string(), "USD")
    );
    assert_eq!(totals.api_unpriced_records, 2);
    assert_eq!(totals.usage.priced_cost.nanos(), 1_400_000_000);
    assert_eq!(totals.usage.unpriced_records, 2);
}

/// Usage priced at `nanos` billionths of a dollar.
fn priced_nanos(nanos: i64) -> AiBillingUsage {
    AiBillingUsage {
        priced_cost: AiUsdNanos::new(nanos).unwrap(),
        ..priced(0, 1)
    }
}

#[test]
fn api_lines_bill_whole_cents_and_totals_add_up_the_billed_lines() {
    // Three API lines of $0.004 each: every line bills $0.00, so the month
    // bills $0.00, while their API-equivalent estimate is $0.012.
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), priced_nanos(4_000_000)),
            claude(date!(2026 - 09 - 03), client_b(), priced_nanos(4_000_000)),
            claude(date!(2026 - 09 - 03), None, priced_nanos(4_000_000)),
        ],
        &[],
    )
    .unwrap();

    let billed: Vec<Option<String>> = bill
        .lines
        .iter()
        .map(|line| line.billable().map(|amount| amount.to_decimal()))
        .collect();
    assert_eq!(billed, vec![Some("0.00".to_string()); 3]);
    let totals = bill.totals().unwrap();
    assert_eq!(totals.api_billed.to_decimal(), "0.00");
    assert_eq!(totals.usage.priced_cost.nanos(), 12_000_000);

    // Half a cent rounds up; the sum is of the rounded lines, not rounded once.
    let halves = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), priced_nanos(5_000_000)),
            claude(date!(2026 - 09 - 03), client_b(), priced_nanos(5_000_000)),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(halves.totals().unwrap().api_billed.to_decimal(), "0.02");
}

#[test]
fn a_wholly_unpriced_api_line_has_no_billable_amount() {
    let bill = bill_month(
        september(),
        &[
            claude(date!(2026 - 09 - 03), client_a(), unpriced(3, 10)),
            claude(date!(2026 - 09 - 03), client_b(), {
                let mut mixed = priced(25, 1);
                mixed.add(&unpriced(1, 1)).unwrap();
                mixed
            }),
        ],
        &[claude_max(1, date!(2026 - 09 - 20), None)],
    )
    .unwrap();

    let billed: Vec<(String, Option<String>)> = bill
        .lines
        .iter()
        .map(|line| {
            (
                summary(line).0,
                line.billable().map(|amount| amount.to_decimal()),
            )
        })
        .collect();
    assert_eq!(
        billed,
        [
            ("Overhead".to_string(), Some("403.33".to_string())),
            ("Client A".to_string(), None),
            ("Client B".to_string(), Some("0.25".to_string())),
        ]
    );
    let totals = bill.totals().unwrap();
    assert_eq!(totals.api_billed.to_decimal(), "0.25");
    assert_eq!(totals.api_unpriced_records, 4);
}

#[test]
fn usd_estimates_are_exact_and_format_rounded_half_up() {
    let usd = AiUsdNanos::new(12_345_650_000).unwrap();
    assert_eq!(usd.to_decimal(2), "12.35");
    assert_eq!(usd.to_decimal(4), "12.3457");
    assert_eq!(usd.to_decimal(0), "12");
    assert_eq!(AiUsdNanos::new(4_999_999).unwrap().to_decimal(2), "0.00");
    assert_eq!(AiUsdNanos::new(5_000_000).unwrap().to_decimal(2), "0.01");
    assert_eq!(AiUsdNanos::ZERO.to_decimal(4), "0.0000");
    assert!((usd.to_usd() - 12.34565).abs() < 1e-12);
    assert_eq!(AiUsdNanos::new(-1), None);
    let largest = AiUsdNanos::new(i64::MAX).unwrap();
    assert_eq!(largest.to_decimal(2), "9223372036.85");
    assert_eq!(largest.to_cents(), 922_337_203_685);
    assert_eq!(largest.to_decimal(9), "9223372036.854775807");
}

#[test]
fn unsupported_monthly_totals_are_errors_instead_of_saturated_bills() {
    let large_cost = priced_nanos(6_000_000_000_000_000_000);
    // Each day and line fits individually; either a project's monthly cost or
    // the total across projects can still exceed the billing representation.
    for second_project in [client_a(), client_b()] {
        let rows = [
            claude(date!(2026 - 09 - 01), client_a(), large_cost),
            claude(date!(2026 - 09 - 02), second_project, large_cost),
        ];
        assert!(matches!(
            bill_month(september(), &rows, &[]),
            Err(AiBillingError::NumericRange)
        ));
    }
    let safe_day = priced(1, 5_000_000_000_000_000);
    let rows = [
        claude(date!(2026 - 09 - 01), client_a(), safe_day),
        claude(date!(2026 - 09 - 02), client_b(), safe_day),
    ];
    assert!(matches!(
        bill_month(september(), &rows, &[]),
        Err(AiBillingError::NumericRange)
    ));
    assert!(matches!(
        allocate_largest_remainder(2, &[u128::MAX]),
        Err(AiBillingError::NumericRange)
    ));
}
