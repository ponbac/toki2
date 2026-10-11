use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{
    bill_month, AiBillingUsage, AiDailyUsage, AiMappedProject, AiMonthlyCost, AiProvider,
    AiSubscription, AiSubscriptionId, AiSubscriptionPeriod, AiSubscriptionPlan,
    AiSubscriptionTerms, AiTokenCounts, AiUsdNanos, ProjectId, UserId,
};

const DEV: UserId = UserId::new(7);

fn september() -> AiBillingMonth {
    AiBillingMonth::parse("2026-09").unwrap()
}

fn fetched(value: &str, provisional: bool, fetched_at: OffsetDateTime) -> AiFetchedExchangeRate {
    AiFetchedExchangeRate {
        rate: AiExchangeRateValue::parse(value).unwrap(),
        provider: "example".to_string(),
        provisional,
        observed_through: date!(2026 - 09 - 29),
        fetched_at,
        fetched_on: fetched_at.date(),
    }
}

fn rate(currency: &str, value: &str) -> AiExchangeRate {
    AiExchangeRate {
        month: september(),
        currency: AiCurrency::parse(currency).unwrap(),
        fetched: Some(fetched(value, false, datetime!(2026-10-01 10:00 UTC))),
        overridden: None,
    }
}

fn provisional(fetched_at: OffsetDateTime, observed_through: Date) -> AiExchangeRate {
    AiExchangeRate {
        fetched: Some(AiFetchedExchangeRate {
            observed_through,
            ..fetched("10", true, fetched_at)
        }),
        ..rate("USD", "10")
    }
}

fn overridden() -> AiExchangeRate {
    AiExchangeRate {
        overridden: Some(AiExchangeRateOverride {
            rate: AiExchangeRateValue::parse("10.5").unwrap(),
            set_on: date!(2026 - 10 - 02),
            by: Some("Admin Example".to_string()),
        }),
        ..rate("USD", "10")
    }
}

fn clock(now: OffsetDateTime) -> AiRateClock {
    AiRateClock {
        now,
        today: now.date(),
    }
}

fn project(id: &str, name: &str) -> Option<AiMappedProject> {
    Some(AiMappedProject {
        id: ProjectId::new(id),
        name: name.to_string(),
    })
}

fn usage(
    provider: AiProvider,
    day: Date,
    project: Option<AiMappedProject>,
    cents: i64,
) -> AiDailyUsage {
    AiDailyUsage {
        user_id: DEV,
        provider,
        day,
        project,
        usage: AiBillingUsage {
            tokens: AiTokenCounts {
                input: 100,
                ..AiTokenCounts::default()
            },
            records: 1,
            priced_cost: AiUsdNanos::new(cents * 10_000_000).unwrap(),
            unpriced_records: 0,
        },
    }
}

fn subscription(id: i32, fee: &str, currency: &str) -> AiSubscription {
    AiSubscription {
        id: AiSubscriptionId::new(id),
        user_id: DEV,
        terms: AiSubscriptionTerms {
            provider: AiProvider::Claude,
            plan: AiSubscriptionPlan::parse("Claude Max 5x").unwrap(),
            monthly_cost: AiMonthlyCost::parse(fee).unwrap(),
            currency: AiCurrency::parse(currency).unwrap(),
            period: AiSubscriptionPeriod::new(date!(2026 - 09 - 01), None).unwrap(),
        },
    }
}

fn amounts(converted: &[AiConvertedAmount]) -> Vec<Option<String>> {
    converted
        .iter()
        .map(|converted| converted.amount().map(AiFeeAmount::to_decimal))
        .collect()
}

#[test]
fn rates_parse_as_exact_decimals_within_bounds() {
    for (raw, millionths, shown) in [
        ("9.50843", 9_508_430, "9.50843"),
        ("0.073704", 73_704, "0.073704"),
        ("11", 11_000_000, "11.00"),
        ("10.5", 10_500_000, "10.50"),
        ("0.000001", 1, "0.000001"),
        ("1000000", 1_000_000_000_000, "1000000.00"),
    ] {
        let rate = AiExchangeRateValue::parse(raw).unwrap();
        assert_eq!(
            (rate.millionths(), rate.to_decimal().as_str()),
            (millionths, shown)
        );
    }
    for raw in [
        "",
        "0",
        "0.000000",
        "-1",
        "1.",
        ".5",
        "1.0000001",
        "1e3",
        "1,5",
        " 1",
        "NaN",
        "1000000.000001",
        "9999999",
    ] {
        assert_eq!(AiExchangeRateValue::parse(raw), None, "{raw:?}");
    }
}

#[test]
fn conversion_rounds_half_up_to_hundredths() {
    let rate = AiExchangeRateValue::parse("9.5").unwrap();
    // 0.01 × 9.5 = 0.095 → 0.10; 0.03 × 9.5 = 0.285 → 0.29.
    assert_eq!(rate.convert(1), Some(10));
    assert_eq!(rate.convert(3), Some(29));
    assert_eq!(rate.convert(0), Some(0));
    assert_eq!(rate.convert(-1), None);
    let rate = AiExchangeRateValue::parse("9.50843").unwrap();
    // 199.00 USD × 9.50843 = 1892.17757 → 1892.18.
    assert_eq!(rate.convert(19_900), Some(189_218));
}

#[test]
fn a_mean_rounds_half_up_to_a_millionth() {
    let values: Vec<_> = ["9.58973", "9.63681", "9.5777"]
        .iter()
        .map(|raw| AiExchangeRateValue::parse(raw).unwrap())
        .collect();
    // (9.58973 + 9.63681 + 9.5777) / 3 = 9.60141333…
    assert_eq!(
        AiExchangeRateValue::mean(&values).unwrap().to_decimal(),
        "9.601413"
    );
    assert_eq!(AiExchangeRateValue::mean(&[]), None);
}

#[test]
fn an_override_takes_precedence_and_keeps_the_fetched_rate() {
    let rate = overridden();
    assert_eq!(rate.rate().unwrap().to_decimal(), "10.50");
    assert_eq!(rate.source(), "admin");
    assert!(!rate.is_provisional());
    assert_eq!(rate.fetched.as_ref().unwrap().rate.to_decimal(), "10.00");
    let fetched_only = AiExchangeRate {
        overridden: None,
        ..rate
    };
    assert_eq!(fetched_only.rate().unwrap().to_decimal(), "10.00");
    assert_eq!(fetched_only.source(), "example");
}

const ALWAYS: fn(Date) -> bool = |_| true;
const NEVER: fn(Date) -> bool = |_| false;

#[test]
fn past_months_fetch_a_final_average_once() {
    let month = september();
    let now = clock(datetime!(2026-10-05 10:00 UTC));
    assert_eq!(
        rate_to_fetch(None, month, now, ALWAYS),
        Some(AiRateFetch::MonthAverage)
    );
    assert_eq!(
        rate_to_fetch(Some(&rate("USD", "10")), month, now, ALWAYS),
        None
    );
    // A month-to-date rate is replaced as soon as the month is over.
    let fetched_during_the_month =
        provisional(datetime!(2026-09-30 21:00 UTC), date!(2026 - 09 - 30));
    assert_eq!(
        rate_to_fetch(
            Some(&fetched_during_the_month),
            month,
            clock(datetime!(2026-10-01 00:05 UTC)),
            NEVER
        ),
        Some(AiRateFetch::MonthAverage)
    );
}

#[test]
fn an_unpublished_past_average_is_retried_hourly() {
    let month = september();
    let fetched = provisional(datetime!(2026-10-01 07:00 UTC), date!(2026 - 09 - 30));
    for (now, fetch) in [
        (datetime!(2026-10-01 07:59 UTC), None),
        (
            datetime!(2026-10-01 08:00 UTC),
            Some(AiRateFetch::MonthAverage),
        ),
    ] {
        assert_eq!(
            rate_to_fetch(Some(&fetched), month, clock(now), NEVER),
            fetch
        );
    }
}

#[test]
fn the_current_month_is_refreshed_once_a_newer_day_may_be_published() {
    let month = september();
    // Fetched at 09:00 on the 29th, with rates through the 28th.
    let fetched = provisional(datetime!(2026-09-29 09:00 UTC), date!(2026 - 09 - 28));
    let later = clock(datetime!(2026-09-29 14:00 UTC));
    let through_today = Some(AiRateFetch::MonthToDate {
        through: date!(2026 - 09 - 29),
    });
    // Not before an hour has passed, whatever was published.
    assert_eq!(
        rate_to_fetch(
            Some(&fetched),
            month,
            clock(datetime!(2026-09-29 09:59 UTC)),
            ALWAYS
        ),
        None
    );
    // Asks whether a day after the 28th may be published.
    let asked = std::cell::Cell::new(None);
    let may_have_published = |day| {
        asked.set(Some(day));
        day < date!(2026 - 09 - 29)
    };
    assert_eq!(
        rate_to_fetch(Some(&fetched), month, later, may_have_published),
        through_today
    );
    assert_eq!(asked.get(), Some(date!(2026 - 09 - 28)));
    // Nothing newer can be published yet, such as on a weekend.
    assert_eq!(rate_to_fetch(Some(&fetched), month, later, NEVER), None);
    assert_eq!(rate_to_fetch(None, month, later, NEVER), through_today);
    // Future months have no rates to fetch.
    assert_eq!(
        rate_to_fetch(None, month, clock(datetime!(2026-08-31 12:00 UTC)), ALWAYS),
        None
    );
}

#[test]
fn an_override_is_never_refetched_beneath() {
    for now in [
        datetime!(2026-09-15 12:00 UTC),
        datetime!(2026-10-05 12:00 UTC),
        datetime!(2027-01-01 12:00 UTC),
    ] {
        let mut stored = overridden();
        stored.fetched = None;
        assert_eq!(
            rate_to_fetch(Some(&stored), september(), clock(now), ALWAYS),
            None
        );
        stored.fetched = Some(fetched("10", true, datetime!(2026-09-02 10:00 UTC)));
        assert_eq!(
            rate_to_fetch(Some(&stored), september(), clock(now), ALWAYS),
            None
        );
    }
}

#[test]
fn subscription_shares_split_the_converted_fee_exactly() {
    let month = september();
    // 199 USD all September, split $3 : $1 : $0.50 as in the original fee.
    let bill = bill_month(
        month,
        &[
            usage(
                AiProvider::Claude,
                date!(2026 - 09 - 02),
                project("101", "Client A"),
                300,
            ),
            usage(
                AiProvider::Claude,
                date!(2026 - 09 - 03),
                project("202", "Client B"),
                100,
            ),
            usage(AiProvider::Claude, date!(2026 - 09 - 04), None, 50),
        ],
        &[subscription(1, "199", "USD")],
    )
    .unwrap();
    let original: Vec<_> = bill
        .lines
        .iter()
        .map(|line| line.billable().unwrap().to_decimal())
        .collect();
    assert_eq!(original, ["132.67", "44.22", "22.11"]);

    let converted = convert_bill(&bill, &[rate("USD", "9.50843")], &[]).unwrap();
    // 199.00 × 9.50843 = 1892.18 SEK, split 3 : 1 : 0.5 → 1261.453…,
    // 420.484…, 210.242…; the left-over öre goes to the largest remainder,
    // Client B's. Converting each USD share instead would give 1261.48 +
    // 420.46 + 210.23 = 1892.17.
    assert_eq!(
        amounts(&converted.subscriptions),
        [Some("1892.18".to_string())]
    );
    assert_eq!(
        amounts(&converted.lines),
        [
            Some("1261.45".to_string()),
            Some("420.49".to_string()),
            Some("210.24".to_string())
        ]
    );
    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "1892.18");
    assert_eq!(converted.totals.fees.unwrap().to_decimal(), "1892.18");
    assert_eq!(converted.totals.api.unwrap().to_decimal(), "0.00");
    assert!(converted.missing_rates.is_empty());
}

/// Splits in both currencies use the weights `bill_month` recorded, for
/// each basis: the original shares are exactly the split of the original
/// fee by those weights, and the converted shares the split of the
/// converted fee by the same weights.
#[test]
fn both_currencies_split_by_the_same_recorded_weights() {
    let month = september();
    let mut all_unpriced = usage(AiProvider::Claude, date!(2026 - 09 - 02), None, 0);
    all_unpriced.usage.unpriced_records = 1;
    let mut more_tokens = all_unpriced.clone();
    more_tokens.project = project("101", "Client A");
    more_tokens.usage.tokens.input = 300;
    let mut no_tokens = all_unpriced.clone();
    no_tokens.usage.tokens.input = 0;
    let mut more_records = no_tokens.clone();
    more_records.project = project("101", "Client A");
    more_records.usage.records = 3;
    for (rows, basis) in [
        (
            vec![
                usage(
                    AiProvider::Claude,
                    date!(2026 - 09 - 02),
                    project("101", "A"),
                    7,
                ),
                usage(AiProvider::Claude, date!(2026 - 09 - 02), None, 3),
            ],
            crate::domain::models::AiAllocationBasis::ApiCost,
        ),
        (
            vec![more_tokens, all_unpriced],
            crate::domain::models::AiAllocationBasis::Tokens,
        ),
        (
            vec![more_records, no_tokens],
            crate::domain::models::AiAllocationBasis::Records,
        ),
    ] {
        let bill = bill_month(month, &rows, &[subscription(1, "33.33", "EUR")]).unwrap();
        let subscription = &bill.subscriptions[0];
        assert_eq!(subscription.basis, Some(basis));
        let original: Vec<i64> = bill
            .lines
            .iter()
            .map(|line| line.billable().unwrap().hundredths)
            .collect();
        assert_eq!(
            allocate_largest_remainder(3_333, &subscription.weights)
                .unwrap()
                .unwrap(),
            original,
            "{basis:?}"
        );

        let converted = convert_bill(&bill, &[rate("EUR", "11.23")], &[]).unwrap();
        let fee = converted.subscriptions[0].amount().unwrap().hundredths;
        let shares: Vec<i64> = converted
            .lines
            .iter()
            .map(|line| line.amount().unwrap().hundredths)
            .collect();
        assert_eq!(
            allocate_largest_remainder(fee, &subscription.weights)
                .unwrap()
                .unwrap(),
            shares,
            "{basis:?}"
        );
    }
}

#[test]
fn the_sek_split_sums_exactly_for_many_fees_and_weights() {
    let month = september();
    for case in 0..200_i64 {
        let fee = format!("{}.{:02}", 1 + case * 7 % 400, case * 13 % 100);
        let rate_value = format!("{}.{:05}", 8 + case % 5, case * 7919 % 100_000);
        let rows: Vec<AiDailyUsage> = (0..(1 + case % 5))
            .map(|index| {
                usage(
                    AiProvider::Claude,
                    date!(2026 - 09 - 01),
                    project(&index.to_string(), &format!("P{index}")),
                    1 + (case * 31 + index * 17) % 97,
                )
            })
            .collect();
        let bill = bill_month(month, &rows, &[subscription(1, &fee, "EUR")]).unwrap();
        let converted = convert_bill(&bill, &[rate("EUR", &rate_value)], &[]).unwrap();
        let fee = converted.subscriptions[0].amount().unwrap().hundredths;
        let shares: i64 = converted
            .lines
            .iter()
            .map(|line| line.amount().unwrap().hundredths)
            .sum();
        assert_eq!(shares, fee, "fee {fee} at {rate_value}");
        assert_eq!(converted.totals.billed.as_ref().unwrap().hundredths, fee);
    }
}

#[test]
fn api_lines_convert_their_billed_cents_and_totals_add_the_lines() {
    let month = september();
    let bill = bill_month(
        month,
        &[
            // $0.03 and $0.05: 0.285 → 0.29 and 0.475 → 0.48 SEK at 9.5.
            usage(
                AiProvider::Codex,
                date!(2026 - 09 - 02),
                project("101", "Client A"),
                3,
            ),
            usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 5),
        ],
        &[],
    )
    .unwrap();
    let converted = convert_bill(&bill, &[rate("USD", "9.5")], &[]).unwrap();
    assert_eq!(
        amounts(&converted.lines),
        [Some("0.29".to_string()), Some("0.48".to_string())]
    );
    // The sum of the rounded lines, not 0.08 × 9.5 = 0.76.
    assert_eq!(converted.totals.api.unwrap().to_decimal(), "0.77");
    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "0.77");
}

#[test]
fn billing_currency_amounts_need_no_rate() {
    let bill = bill_month(
        september(),
        &[usage(AiProvider::Claude, date!(2026 - 09 - 02), None, 100)],
        &[subscription(1, "1100", "SEK")],
    )
    .unwrap();
    let converted = convert_bill(&bill, &[], &[]).unwrap();
    assert_eq!(amounts(&converted.lines), [Some("1100.00".to_string())]);
    assert_eq!(converted.lines[0].rate(), None);
    assert!(converted.missing_rates.is_empty());
    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "1100.00");
}

#[test]
fn zero_amounts_need_no_rate() {
    // A USD line billed at $0.00 and a 0 EUR fee: zero SEK, no rate asked for.
    let bill = bill_month(
        september(),
        &[
            usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 0),
            usage(AiProvider::Claude, date!(2026 - 09 - 02), None, 5),
        ],
        &[subscription(1, "0", "EUR")],
    )
    .unwrap();
    assert!(currencies_to_convert(&bill).is_empty());
    let converted = convert_bill(&bill, &[], &[]).unwrap();
    assert_eq!(
        amounts(&converted.lines),
        [Some("0.00".to_string()), Some("0.00".to_string())]
    );
    assert!(converted.missing_rates.is_empty());
    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "0.00");
    assert_eq!(
        amounts(&converted.subscriptions),
        [Some("0.00".to_string())]
    );
}

#[test]
fn a_missing_rate_leaves_amounts_unknown_never_zero() {
    let month = september();
    let bill = bill_month(
        month,
        &[
            usage(AiProvider::Claude, date!(2026 - 09 - 02), None, 100),
            usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 100),
        ],
        &[
            subscription(1, "20", "EUR"),
            AiSubscription {
                terms: AiSubscriptionTerms {
                    provider: AiProvider::Copilot,
                    ..subscription(2, "300", "SEK").terms
                },
                ..subscription(2, "300", "SEK")
            },
        ],
    )
    .unwrap();
    let eur = AiCurrency::parse("EUR").unwrap();
    // USD is known, EUR is not, and is still being fetched.
    let converted = convert_bill(&bill, &[rate("USD", "10")], std::slice::from_ref(&eur)).unwrap();
    let charges: Vec<_> = bill
        .lines
        .iter()
        .zip(&converted.lines)
        .map(|(line, converted)| (line.provider, converted.clone()))
        .collect();
    assert!(charges
        .iter()
        .any(|(provider, converted)| *provider == AiProvider::Claude
            && *converted
                == AiConvertedAmount::MissingRate {
                    currency: eur.clone()
                }));
    assert!(charges
        .iter()
        .any(|(provider, converted)| *provider == AiProvider::Codex
            && converted.amount().map(AiFeeAmount::to_decimal) == Some("10.00".to_string())));
    assert!(charges
        .iter()
        .any(|(provider, converted)| *provider == AiProvider::Copilot
            && converted.amount().map(AiFeeAmount::to_decimal) == Some("300.00".to_string())));
    assert_eq!(converted.missing_rates, std::slice::from_ref(&eur));
    assert_eq!(converted.pending_rates, std::slice::from_ref(&eur));
    // Every total that includes the EUR line is unknown; the rest are known.
    assert_eq!(converted.totals.billed, None);
    assert_eq!(converted.totals.fees, None);
    assert_eq!(converted.totals.api.unwrap().to_decimal(), "10.00");
    assert_eq!(converted.totals.overhead.unwrap().to_decimal(), "300.00");
    assert_eq!(
        converted.subscriptions[0],
        AiConvertedAmount::MissingRate { currency: eur }
    );
}

#[test]
fn a_provisional_rate_refresh_is_pending_even_with_usable_amounts() {
    let bill = bill_month(
        september(),
        &[usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 100)],
        &[],
    )
    .unwrap();
    let mut provisional = rate("USD", "10");
    provisional.fetched.as_mut().unwrap().provisional = true;
    let pending = [AiCurrency::usd(), AiCurrency::parse("EUR").unwrap()];
    let converted = convert_bill(&bill, &[provisional], &pending).unwrap();

    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "10.00");
    assert!(converted.missing_rates.is_empty());
    // Only an active fetch needed by the bill should keep its page polling.
    assert_eq!(converted.pending_rates, [AiCurrency::usd()]);
    assert!(convert_bill(&bill, &[], &[])
        .unwrap()
        .pending_rates
        .is_empty());
}

#[test]
fn converted_amounts_and_totals_reject_numeric_overflow() {
    let mut large_fee = subscription(1, "1", "EUR");
    large_fee.terms.monthly_cost = AiMonthlyCost::from_hundredths(10_000_000_000_000).unwrap();
    let large_total: Vec<_> = (1..=10)
        .map(|id| AiSubscription {
            user_id: UserId::new(id),
            ..subscription(id, "9999999999.99", "EUR")
        })
        .collect();

    // Both original bills fit their currencies. The first converted amount
    // overflows; in the second, individual amounts fit but their sum does not.
    for subscriptions in [vec![large_fee], large_total] {
        let bill = bill_month(september(), &[], &subscriptions).unwrap();
        let ordinary = convert_bill(&bill, &[rate("EUR", "1")], &[]).unwrap();
        assert_eq!(
            ordinary.totals.billed.as_ref().unwrap().hundredths,
            bill.totals().unwrap().fees[0].billed.hundredths
        );
        assert!(matches!(
            convert_bill(&bill, &[rate("EUR", "1000000")], &[]),
            Err(AiBillingError::NumericRange)
        ));
    }
}

#[test]
fn unknown_api_amounts_stay_unknown_without_hiding_the_totals() {
    let mut row = usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 0);
    row.usage.unpriced_records = 1;
    let bill = bill_month(september(), &[row], &[]).unwrap();
    let converted = convert_bill(&bill, &[], &[]).unwrap();
    assert_eq!(converted.lines, [AiConvertedAmount::UnknownAmount]);
    assert!(converted.missing_rates.is_empty());
    assert_eq!(converted.totals.billed.unwrap().to_decimal(), "0.00");
}

#[test]
fn an_override_converts_like_any_rate_and_other_months_rates_are_ignored() {
    let bill = bill_month(
        september(),
        &[usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 100)],
        &[],
    )
    .unwrap();
    let august = AiExchangeRate {
        month: AiBillingMonth::parse("2026-08").unwrap(),
        ..rate("USD", "99")
    };
    let converted = convert_bill(&bill, &[august, overridden()], &[]).unwrap();
    assert_eq!(amounts(&converted.lines), [Some("10.50".to_string())]);
    assert!(converted.lines[0].rate().unwrap().is_override());
    assert_eq!(converted.rates, [overridden()]);
}

#[test]
fn only_non_zero_amounts_in_other_currencies_need_converting() {
    let bill = bill_month(
        september(),
        &[usage(AiProvider::Codex, date!(2026 - 09 - 02), None, 100)],
        &[subscription(1, "1100", "SEK")],
    )
    .unwrap();
    assert_eq!(currencies_to_convert(&bill), [AiCurrency::usd()]);
}
