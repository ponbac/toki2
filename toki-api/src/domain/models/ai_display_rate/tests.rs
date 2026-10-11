use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{AiExchangeRateOverride, AiFetchedExchangeRate};

fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
}

fn rate(month: AiBillingMonth, currency: &str, value: &str, provisional: bool) -> AiExchangeRate {
    AiExchangeRate {
        month,
        currency: AiCurrency::parse(currency).unwrap(),
        fetched: Some(AiFetchedExchangeRate {
            rate: AiExchangeRateValue::parse(value).unwrap(),
            provider: "example".to_string(),
            provisional,
            observed_through: date!(2026 - 10 - 09),
            fetched_at: datetime!(2026-10-11 10:00 UTC),
            fetched_on: date!(2026 - 10 - 11),
        }),
        overridden: None,
    }
}

fn shown(rates: &[AiDisplayRate]) -> Vec<(&str, String, bool, bool)> {
    rates
        .iter()
        .map(|rate| {
            (
                rate.currency.as_str(),
                rate.per_usd.to_decimal(),
                rate.provisional,
                rate.overridden,
            )
        })
        .collect()
}

#[test]
fn a_range_shows_at_the_month_of_its_last_day_but_never_a_month_to_come() {
    let today = date!(2026 - 10 - 11);
    let month = |through| display_rate_month(today, through).unwrap().to_string();

    assert_eq!(month(None), "2026-10");
    assert_eq!(month(Some(date!(2026 - 08 - 31))), "2026-08");
    assert_eq!(month(Some(date!(2026 - 10 - 31))), "2026-10");
    assert_eq!(month(Some(date!(2027 - 01 - 15))), "2026-10");
}

#[test]
fn a_dollar_shows_in_the_billing_currency_and_across_it_in_others() {
    let rates = [
        rate(october(), "USD", "9.42", false),
        rate(october(), "EUR", "11.02", true),
    ];

    // 9.42 SEK per USD over 11.02 SEK per EUR is 0.8548094... EUR per USD.
    assert_eq!(
        shown(&display_rates(october(), &rates)),
        [
            ("SEK", "9.42".to_string(), false, false),
            ("EUR", "0.854809".to_string(), true, false),
        ]
    );
}

#[test]
fn an_override_is_the_rate_shown_at() {
    let mut usd = rate(october(), "USD", "9.42", true);
    usd.overridden = Some(AiExchangeRateOverride {
        rate: AiExchangeRateValue::parse("10").unwrap(),
        set_on: date!(2026 - 10 - 02),
        by: None,
    });

    assert_eq!(
        shown(&display_rates(october(), &[usd])),
        [("SEK", "10.00".to_string(), false, true)]
    );
}

#[test]
fn nothing_converts_without_the_months_usd_rate() {
    let september = october().previous().unwrap();
    let rates = [
        rate(september, "USD", "9.50", false),
        rate(october(), "EUR", "11.02", true),
    ];

    assert_eq!(display_rates(october(), &rates), []);
    assert_eq!(
        shown(&display_rates(september, &rates)),
        [("SEK", "9.50".to_string(), false, false)]
    );
}

#[test]
fn the_month_before_january_is_december() {
    let january = AiBillingMonth::parse("2026-01").unwrap();

    assert_eq!(january.previous().unwrap().to_string(), "2025-12");
    assert_eq!(AiBillingMonth::parse("0001-01").unwrap().previous(), None);
}
