use super::*;

fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap_or_else(|| panic!("{raw} should parse"))
}

#[test]
fn decimals_parse_exactly_into_canonical_form() {
    for (raw, shown) in [
        ("0", "0"),
        ("0.0", "0"),
        ("19.0", "19"),
        ("12.500", "12.5"),
        ("3.12e-05", "0.0000312"),
        ("1e-05", "0.00001"),
        ("1E+3", "1000"),
        ("1.5e3", "1500"),
        ("0.0012", "0.0012"),
        ("12.345678912", "12.345678912"),
        ("100", "100"),
    ] {
        assert_eq!(dec(raw).to_string(), shown, "{raw}");
    }
    assert_eq!(dec("19.0"), dec("19"));
    assert!(dec("0.0").is_zero());
}

#[test]
fn decimals_refuse_signs_blanks_strings_and_too_many_digits() {
    for raw in [
        "-1",
        "+1",
        "",
        " 1",
        "\"1.5\"",
        "1.",
        ".5",
        "1e",
        "abc",
        "null",
        "1e99999",
        // 31 significant digits.
        "1234567890123456789012345678901",
        // More than 36 decimals.
        "0.0000000000000000000000000000000000001",
    ] {
        assert_eq!(AiDecimal::parse(raw), None, "{raw:?}");
    }
    // 30 significant digits parse.
    assert!(AiDecimal::parse("12345678901234567890.1234567891").is_some());
}

#[test]
fn float_artifacts_parse_exactly() {
    // 19 decimals, as GitHub writes some amounts.
    let artifact = dec("0.0034338949999999993");
    assert_eq!(artifact.to_string(), "0.0034338949999999993");
    assert_eq!(artifact.to_fixed(2), "0.00");
    assert!(artifact.matches(&dec("0.003433895")));
    assert!(artifact < dec("0.003433895"));
    assert_eq!(
        artifact.checked_add(&dec("1")).unwrap().to_string(),
        "1.0034338949999999993"
    );
    assert_eq!(
        dec("1.2345678901234567e-15").to_string(),
        "0.0000000000000012345678901234567"
    );
}

#[test]
fn sums_overflow_to_none() {
    let large = dec("90000000000000000000000000000");
    let tiny = dec("0.000000000000000000000000000001");
    assert_eq!(large.checked_add(&tiny), None);
    assert!(large > tiny);
    assert!(!large.matches(&tiny));
    assert_eq!(
        dec("0.1").checked_add(&dec("0.25")).unwrap().to_string(),
        "0.35"
    );
    assert_eq!(
        AiDecimal::sum([dec("1.5"), dec("2.5"), dec("0.000001")].iter())
            .unwrap()
            .to_string(),
        "4.000001"
    );
}

#[test]
fn credits_become_nanodollars_rounded_half_up() {
    // 9 decimals: 12.345678912 credits are $0.12345678912.
    assert_eq!(
        dec("12.345678912").credits_to_usd_nanos(),
        Some(123_456_789)
    );
    // 0.00000005 credits are half a nanodollar: up.
    assert_eq!(dec("0.00000005").credits_to_usd_nanos(), Some(1));
    assert_eq!(dec("0.000000049").credits_to_usd_nanos(), Some(0));
    // 1,500 credits are $15.
    assert_eq!(dec("1500").credits_to_usd_nanos(), Some(15 * 1_000_000_000));
    assert_eq!(dec("1").credits_to_usd_nanos(), Some(NANOS_PER_CREDIT));
    // Beyond i64 nanodollars.
    assert_eq!(dec("10000000000000").credits_to_usd_nanos(), None);
}

#[test]
fn fixed_decimals_round_half_up() {
    assert_eq!(dec("12.345").to_fixed(2), "12.35");
    assert_eq!(dec("12.344").to_fixed(2), "12.34");
    assert_eq!(dec("7").to_fixed(2), "7.00");
    assert_eq!(dec("0.005").to_fixed(2), "0.01");
    assert_eq!(dec("0.0000312").to_fixed(2), "0.00");
    assert_eq!(dec("12.5").to_fixed(0), "13");
    assert_eq!(dec("1e38").to_fixed(2), format!("1{}.00", "0".repeat(38)));
}

#[test]
fn decimals_order_by_value() {
    assert!(dec("0.1") < dec("0.25"));
    assert!(dec("10") > dec("9.999999"));
    assert_eq!(dec("1.50").cmp(&dec("1.5")), Ordering::Equal);
}

fn total(label: &str, quantity: &str) -> AiCreditTotal {
    AiCreditTotal {
        key: AiCreditKey {
            product: "Copilot".to_string(),
            sku: "Copilot AI Credits".to_string(),
            label: label.to_string(),
        },
        gross_quantity: dec(quantity),
    }
}

#[test]
fn month_totals_match_within_a_millionth_of_a_credit() {
    let github = [total("GPT-6.1 Sol", "10.000001")];
    assert!(credit_totals_match(&github, &[total("GPT-6.1 Sol", "10")]));
    assert!(!credit_totals_match(
        &[total("GPT-6.1 Sol", "10.000002")],
        &[total("GPT-6.1 Sol", "10")]
    ));
    // A key missing on one side counts as zero.
    assert!(!credit_totals_match(
        &[total("GPT-6.1 Sol", "10"), total("GPT-5.4", "1")],
        &[total("GPT-6.1 Sol", "10")]
    ));
    assert!(credit_totals_match(
        &[total("GPT-6.1 Sol", "10"), total("GPT-5.4", "0")],
        &[total("GPT-6.1 Sol", "10")]
    ));
    assert!(credit_totals_match(&[], &[]));
}

fn credit(label: &str, model: &str, gross: &str, unit_price: &str) -> AiMeteredCredit {
    AiMeteredCredit {
        product: "Copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "ai-credits".to_string(),
        model: AiMeteredModel {
            label: label.to_string(),
            model: model.to_string(),
            auto_routed: false,
        },
        kind: AiCreditKind::Trackable,
        amounts: AiMeteredAmounts {
            unit_price: dec(unit_price),
            gross_quantity: dec(gross),
            gross_amount: dec(gross),
            net_quantity: dec(gross),
            net_amount: dec(gross),
            ..AiMeteredAmounts::default()
        },
    }
}

#[test]
fn duplicate_keys_merge_only_with_equal_units_and_prices() {
    let merged = merge_credits(vec![
        credit("GPT-6.1 Sol", "gpt-6.1-sol", "1.5", "0.01"),
        credit("GPT-5.4", "gpt-5.4", "2", "0.01"),
        credit("GPT-6.1 Sol", "gpt-6.1-sol", "2.25", "0.01"),
    ])
    .unwrap();
    assert_eq!(merged.len(), 2);
    let sol = merged
        .iter()
        .find(|row| row.model.label == "GPT-6.1 Sol")
        .unwrap();
    assert_eq!(sol.amounts.gross_quantity, dec("3.75"));
    assert_eq!(sol.amounts.unit_price, dec("0.01"));
    assert_eq!(sol.gross_usd().unwrap().nanos(), 37_500_000);

    let error = merge_credits(vec![
        credit("GPT-6.1 Sol", "gpt-6.1-sol", "1", "0.01"),
        credit("GPT-6.1 Sol", "gpt-6.1-sol", "1", "0.02"),
    ])
    .unwrap_err();
    assert!(error.contains("GPT-6.1 Sol"), "{error}");

    let totals = credit_totals(&merged).unwrap();
    assert_eq!(total_gross_quantity(&totals), Some(dec("5.75")));
}

fn charge(day: time::Date, repository: &str, quantity: &str, at: &str) -> AiMeteredOrgCharge {
    AiMeteredOrgCharge {
        day,
        product: "copilot".to_string(),
        sku: "Copilot AI Credits".to_string(),
        unit: "AICredits".to_string(),
        repository: repository.to_string(),
        kind: AiOrgChargeKind::RepositoryCredits,
        unit_price: dec("0.01"),
        quantity: dec(quantity),
        gross_amount: dec(quantity),
        discount_amount: AiDecimal::ZERO,
        net_amount: dec(quantity),
        reported_at: Some(
            OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339).unwrap(),
        ),
    }
}

#[test]
fn org_rows_of_one_day_and_repository_merge() {
    let day = time::macros::date!(2026 - 10 - 05);
    let merged = merge_org_charges(vec![
        charge(day, "widget-api", "100", "2026-10-05T09:00:00Z"),
        charge(day, "widget-api", "50", "2026-10-05T06:00:00Z"),
        charge(day, "gadget-web", "7", "2026-10-05T10:00:00Z"),
    ])
    .unwrap();
    assert_eq!(merged.len(), 2);
    let widget = merged
        .iter()
        .find(|row| row.repository == "widget-api")
        .unwrap();
    assert_eq!(widget.quantity, dec("150"));
    assert_eq!(
        widget.reported_at.unwrap(),
        time::macros::datetime!(2026-10-05 06:00 UTC)
    );
    assert_eq!(widget.credit_usd().unwrap().nanos(), 1_500_000_000);

    let mut seats = charge(day, "", "1", "2026-10-05T09:00:00Z");
    seats.kind = AiOrgChargeKind::Seats;
    assert_eq!(seats.credit_usd(), None);
}

#[test]
fn the_copilot_catalog_knows_confirmed_provisional_and_free_models() {
    let status = |model: &str| metered_model_status(AiProvider::Copilot, model);
    assert_eq!(status("gpt-6.1-sol"), Some(AiMeteredModelStatus::Confirmed));
    assert_eq!(
        status("gpt-5.4-mini"),
        Some(AiMeteredModelStatus::Confirmed)
    );
    assert_eq!(
        status("claude-sonnet-5.5"),
        Some(AiMeteredModelStatus::Confirmed)
    );
    assert_eq!(
        status("gpt-6-astra"),
        Some(AiMeteredModelStatus::Provisional)
    );
    assert_eq!(
        status("claude-opus-5.5"),
        Some(AiMeteredModelStatus::Provisional)
    );
    for free in [
        "gpt-4o-mini",
        "gpt-4o-mini-2024-07-18",
        "copilot-nes-x",
        "nes-y",
    ] {
        assert_eq!(status(free), Some(AiMeteredModelStatus::Free), "{free}");
    }
    assert_eq!(status("gpt-4o"), None);
    assert_eq!(status("code-review"), None);
    assert_eq!(status(""), None);
    // Only metered payers have a catalog.
    assert_eq!(metered_model_status(AiProvider::Codex, "gpt-6.1-sol"), None);
}

#[test]
fn kinds_and_sources_round_trip_their_names() {
    for kind in [
        AiCreditKind::Trackable,
        AiCreditKind::Untrackable,
        AiCreditKind::RepositoryBilled,
        AiCreditKind::Unrecognized,
    ] {
        assert_eq!(AiCreditKind::parse(kind.as_str()), Some(kind));
    }
    for kind in [
        AiOrgChargeKind::RepositoryCredits,
        AiOrgChargeKind::AccountCreditsAggregate,
        AiOrgChargeKind::UntrackableCredits,
        AiOrgChargeKind::Compute,
        AiOrgChargeKind::Seats,
        AiOrgChargeKind::Unrecognized,
    ] {
        assert_eq!(AiOrgChargeKind::parse(kind.as_str()), Some(kind));
    }
    for kind in [
        AiSummaryKind::Credits,
        AiSummaryKind::UntrackableCredits,
        AiSummaryKind::Seats,
        AiSummaryKind::Unrecognized,
    ] {
        assert_eq!(AiSummaryKind::parse(kind.as_str()), Some(kind));
    }
    for source in AiMeteredSource::ALL {
        assert_eq!(AiMeteredSource::parse(source.as_str()), Some(source));
    }
}
