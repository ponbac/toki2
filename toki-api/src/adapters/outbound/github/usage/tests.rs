use std::{collections::BTreeMap, time::Duration};

use axum::http::StatusCode;
use time::macros::{date, datetime};

use super::*;
use crate::{
    adapters::outbound::github::{test_support::*, GithubAccountDirectory},
    domain::{
        models::{
            credit_totals, credit_totals_match, AiCreditKind, AiOrgChargeKind, AiProviderAccountId,
            AiProviderLogin, AiSummaryKind,
        },
        ports::outbound::ProviderAccountDirectory,
    },
};

const USER_MONTH: &str = include_str!("../fixtures/ai_credit_user_month.json");
const USER_DAY: &str = include_str!("../fixtures/ai_credit_user_day.json");
const ORG_MONTH: &str = include_str!("../fixtures/ai_credit_org_month.json");
const ORG_DAY: &str = include_str!("../fixtures/ai_credit_org_day.json");
const USAGE_MONTH: &str = include_str!("../fixtures/usage_month.json");
const SUMMARY: &str = include_str!("../fixtures/usage_summary.json");
const ORG: &str = "example-org";

fn october() -> AiBillingMonth {
    AiBillingMonth::parse("2026-10").unwrap()
}

fn ada() -> AiProviderLogin {
    AiProviderLogin::parse("ada-lovelace").unwrap()
}

fn dec(raw: &str) -> AiDecimal {
    AiDecimal::parse(raw).unwrap()
}

fn app(api_base: &str) -> GithubApp {
    GithubApp::new(credentials(api_base, ORG, pkcs1_pem())).unwrap()
}

fn source(api_base: &str) -> GithubMeteredUsageSource {
    GithubMeteredUsageSource::new(app(api_base))
}

const USER_MONTH_PATH: &str = "/organizations/example-org/settings/billing/ai_credit/usage?year=2026&month=10&user=ada-lovelace";
const USER_DAY_PATH: &str = "/organizations/example-org/settings/billing/ai_credit/usage?year=2026&month=10&day=5&user=ada-lovelace";
const ORG_MONTH_PATH: &str =
    "/organizations/example-org/settings/billing/ai_credit/usage?year=2026&month=10";
const ORG_DAY_PATH: &str =
    "/organizations/example-org/settings/billing/ai_credit/usage?year=2026&month=10&day=5";
const USAGE_MONTH_PATH: &str =
    "/organizations/example-org/settings/billing/usage?year=2026&month=10";
const USAGE_DAY_PATH: &str =
    "/organizations/example-org/settings/billing/usage?year=2026&month=10&day=6";
const SUMMARY_PATH: &str =
    "/organizations/example-org/settings/billing/usage/summary?year=2026&month=10&product=copilot";

#[tokio::test]
async fn every_scope_and_period_asks_its_own_path() {
    let (api_base, github) = fake_github().await;
    github.answer(USER_MONTH_PATH, USER_MONTH);
    github.answer(USER_DAY_PATH, USER_DAY);
    github.answer(ORG_MONTH_PATH, ORG_MONTH);
    github.answer(ORG_DAY_PATH, ORG_DAY);
    github.answer(USAGE_MONTH_PATH, USAGE_MONTH);
    github.answer(USAGE_DAY_PATH, r#"{"usageItems": []}"#);
    github.answer(SUMMARY_PATH, SUMMARY);
    let source = source(&api_base);
    let login = ada();
    let day = date!(2026 - 10 - 05);

    source
        .credits(
            AiMeteredScope::Account(&login),
            AiMeteredPeriod::Month(october()),
        )
        .await
        .unwrap();
    source
        .credits(AiMeteredScope::Account(&login), AiMeteredPeriod::Day(day))
        .await
        .unwrap();
    source
        .credits(
            AiMeteredScope::Organization,
            AiMeteredPeriod::Month(october()),
        )
        .await
        .unwrap();
    source
        .credits(AiMeteredScope::Organization, AiMeteredPeriod::Day(day))
        .await
        .unwrap();
    source
        .org_usage(AiMeteredPeriod::Month(october()))
        .await
        .unwrap();
    assert!(source
        .org_usage(AiMeteredPeriod::Day(date!(2026 - 10 - 06)))
        .await
        .unwrap()
        .is_empty());
    source.summary(october()).await.unwrap();

    assert_eq!(
        github.paths(),
        [
            USER_MONTH_PATH,
            USER_DAY_PATH,
            ORG_MONTH_PATH,
            ORG_DAY_PATH,
            USAGE_MONTH_PATH,
            USAGE_DAY_PATH,
            SUMMARY_PATH,
        ]
    );
    for received in github.received() {
        assert_eq!(received.accept, "application/vnd.github+json");
        assert_eq!(received.authorization, "Bearer ghs_synthetic_1");
    }
    // Logins are query-encoded.
    assert_eq!(
        credits_path(
            ORG,
            AiMeteredPeriod::Month(october()),
            Some("odd name&more")
        ),
        "/organizations/example-org/settings/billing/ai_credit/usage?year=2026&month=10&user=odd+name%26more"
    );
}

#[test]
fn labels_map_to_models_and_rows_to_kinds() {
    let login = ada();
    let rows = parse_credits(
        USER_MONTH,
        ORG,
        AiMeteredScope::Account(&login),
        AiMeteredPeriod::Month(october()),
    )
    .unwrap();
    let by_label: BTreeMap<&str, &AiMeteredCredit> = rows
        .iter()
        .filter(|row| row.sku == "Copilot AI Credits" || row.sku == "Copilot Cloud Agent")
        .map(|row| (row.model.label.as_str(), row))
        .collect();
    let kind_of = |label: &str| {
        let row = by_label[label];
        (row.model.model.as_str(), row.model.auto_routed, row.kind)
    };
    assert_eq!(
        kind_of("GPT-6.1 Sol"),
        ("gpt-6.1-sol", false, AiCreditKind::Trackable)
    );
    assert_eq!(
        kind_of("Auto: GPT-6.1 Sol"),
        ("gpt-6.1-sol", true, AiCreditKind::Trackable)
    );
    assert_eq!(
        kind_of("GPT-5.4 mini"),
        ("gpt-5.4-mini", false, AiCreditKind::Trackable)
    );
    assert_eq!(
        kind_of("Code Review model"),
        ("code-review", false, AiCreditKind::RepositoryBilled)
    );
    assert_eq!(
        kind_of("Claude Sonnet 5.5"),
        ("claude-sonnet-5.5", false, AiCreditKind::Untrackable)
    );
    // A wrong unit, a wrong unit price and an empty label are unrecognized,
    // and kept.
    assert_eq!(
        kind_of("GPT-6 Luna"),
        ("gpt-6-luna", false, AiCreditKind::Unrecognized)
    );
    assert_eq!(
        kind_of("GPT-6 Sol"),
        ("gpt-6-sol", false, AiCreditKind::Unrecognized)
    );
    assert_eq!(kind_of(""), ("", false, AiCreditKind::Unrecognized));
    let mystery = rows
        .iter()
        .find(|row| row.sku == "Copilot Mystery Units")
        .unwrap();
    assert_eq!(mystery.kind, AiCreditKind::Unrecognized);
    assert_eq!(rows.len(), 9);
}

#[test]
fn numbers_are_kept_exactly() {
    let login = ada();
    let rows = parse_credits(
        USER_DAY,
        ORG,
        AiMeteredScope::Account(&login),
        AiMeteredPeriod::Day(date!(2026 - 10 - 05)),
    )
    .unwrap();
    let auto = rows.iter().find(|row| row.model.auto_routed).unwrap();
    assert_eq!(auto.amounts.gross_quantity.to_string(), "12.345678912");
    assert_eq!(auto.amounts.discount_amount.to_string(), "0.00001");
    assert_eq!(auto.amounts.unit_price, dec("0.01"));
    // G is the quantity × $0.01, not GitHub's gross amount.
    assert_eq!(auto.gross_usd().unwrap().nanos(), 123_456_789);
    assert_eq!(auto.amounts.gross_amount.to_string(), "0.12345678");
}

#[test]
fn the_month_usage_returns_rows_per_day() {
    let rows = parse_usage(USAGE_MONTH, ORG, AiMeteredPeriod::Month(october())).unwrap();
    let find = |sku: &str, repository: &str| {
        rows.iter()
            .find(|row| row.sku == sku && row.repository == repository)
            .unwrap_or_else(|| panic!("no {sku} {repository:?}"))
    };
    // Two code review rows of one day merge; the earliest instant is kept.
    let review = find("Copilot AI Credits", "widget-api");
    assert_eq!(review.kind, AiOrgChargeKind::RepositoryCredits);
    assert_eq!(review.quantity, dec("200"));
    assert_eq!(review.day, date!(2026 - 10 - 05));
    assert_eq!(review.reported_at, Some(datetime!(2026-10-05 09:12 UTC)));
    assert_eq!(review.credit_usd().unwrap().nanos(), 2_000_000_000);
    assert_eq!(
        find("Copilot AI Credits", "").kind,
        AiOrgChargeKind::AccountCreditsAggregate
    );
    assert_eq!(
        find("Copilot Cloud Agent", "").kind,
        AiOrgChargeKind::UntrackableCredits
    );
    let linux = find("Actions Linux", "gadget-web");
    assert_eq!(linux.kind, AiOrgChargeKind::Compute);
    assert_eq!(linux.credit_usd(), None);
    // A row dated exactly midnight keeps its own day.
    let storage = find("Actions storage", "gadget-web");
    assert_eq!(storage.day, date!(2026 - 10 - 06));
    assert_eq!(storage.reported_at, Some(datetime!(2026-10-06 00:00 UTC)));
    assert_eq!(find("Copilot Business", "").kind, AiOrgChargeKind::Seats);
    assert_eq!(
        find("Copilot Spaces", "").kind,
        AiOrgChargeKind::Unrecognized
    );
    assert_eq!(rows.len(), 7);
}

#[test]
fn the_summary_reads_credits_seats_and_the_pool() {
    let lines = parse_summary(SUMMARY, ORG, october()).unwrap();
    let kinds: Vec<AiSummaryKind> = lines.iter().map(|line| line.kind).collect();
    assert_eq!(
        kinds,
        [
            AiSummaryKind::Credits,
            AiSummaryKind::UntrackableCredits,
            AiSummaryKind::Seats
        ]
    );
    assert_eq!(lines[0].amounts.discount_quantity, dec("3800"));
    let error = parse_summary(
        &SUMMARY.replace(r#""product": "copilot","#, r#""product": "actions","#),
        ORG,
        october(),
    )
    .unwrap_err();
    assert!(matches!(error, GithubError::Response(_)), "{error:?}");
}

fn refused<T: std::fmt::Debug>(result: Result<T, GithubError>) -> String {
    match result {
        Err(GithubError::Response(message)) => message,
        other => panic!("not a response error: {other:?}"),
    }
}

#[test]
fn responses_that_do_not_match_their_request_are_refused() {
    let login = ada();
    let account = AiMeteredScope::Account(&login);
    let month = AiMeteredPeriod::Month(october());
    let day = AiMeteredPeriod::Day(date!(2026 - 10 - 05));

    // Another period: a day answered for a month, another month.
    assert!(refused(parse_credits(USER_DAY, ORG, account, month)).contains("timePeriod"));
    assert!(refused(parse_credits(
        USER_MONTH,
        ORG,
        account,
        AiMeteredPeriod::Month(AiBillingMonth::parse("2026-09").unwrap())
    ))
    .contains("timePeriod"));
    // GitHub ignoring `user` and answering the organization, or someone else.
    assert!(refused(parse_credits(ORG_DAY, ORG, account, day)).contains("instead of"));
    let grace = USER_DAY.replace("ada-lovelace", "grace-hopper");
    assert!(refused(parse_credits(&grace, ORG, account, day)).contains("grace-hopper"));
    // Logins compare case-insensitively.
    let shouted = USER_DAY.replace("ada-lovelace", "Ada-Lovelace");
    assert!(parse_credits(&shouted, ORG, account, day).is_ok());
    // Another organization.
    assert!(refused(parse_credits(USER_DAY, "other-org", account, day)).contains("organization"));
    let other_usage = USAGE_MONTH.replacen(
        r#""organizationName": "example-org""#,
        r#""organizationName": "other-org""#,
        1,
    );
    assert!(refused(parse_usage(&other_usage, ORG, month)).contains("organizationName"));
    // A row outside the requested day.
    assert!(refused(parse_usage(
        USAGE_MONTH,
        ORG,
        AiMeteredPeriod::Day(date!(2026 - 10 - 05))
    ))
    .contains("outside"));
    // Negative and string numbers.
    let negative = USER_DAY.replacen(r#""grossQuantity": 60.0"#, r#""grossQuantity": -60.0"#, 1);
    assert!(refused(parse_credits(&negative, ORG, account, day)).contains("grossQuantity"));
    let string = USER_DAY.replacen(r#""grossQuantity": 60.0"#, r#""grossQuantity": "60.0""#, 1);
    assert!(refused(parse_credits(&string, ORG, account, day)).contains("grossQuantity"));
    let null = USER_DAY.replacen(r#""netAmount": 0.0"#, r#""netAmount": null"#, 1);
    assert!(refused(parse_credits(&null, ORG, account, day)).contains("netAmount"));
    // Two rows of one key with different unit prices cannot merge.
    let conflict = USER_DAY
        .replacen("Auto: GPT-6.1 Sol", "GPT-6.1 Sol", 1)
        .replacen(
            r#""pricePerUnit": 0.01,
      "grossQuantity": 12.345678912"#,
            r#""pricePerUnit": 0.02,
      "grossQuantity": 12.345678912"#,
            1,
        );
    assert!(refused(parse_credits(&conflict, ORG, account, day)).contains("GPT-6.1 Sol"));
    assert!(refused(parse_credits("not json", ORG, account, day)).contains("unreadable"));
}

#[tokio::test]
async fn github_answers_map_to_usage_source_errors() {
    let (api_base, github) = fake_github().await;
    let source = source(&api_base);
    let login = ada();

    github.answer_with(
        USER_MONTH_PATH,
        StatusCode::FORBIDDEN,
        Vec::new(),
        r#"{"message": "Resource not accessible by integration"}"#,
    );
    let forbidden = source
        .credits(
            AiMeteredScope::Account(&login),
            AiMeteredPeriod::Month(october()),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&forbidden, MeteredUsageSourceError::Forbidden(message)
            if message.contains("Administration: read")
                && message.contains("Resource not accessible")),
        "{forbidden}"
    );

    // An unanswered path is a 404, named by scope.
    let missing_user = source
        .credits(
            AiMeteredScope::Account(&login),
            AiMeteredPeriod::Day(date!(2026 - 10 - 01)),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&missing_user, MeteredUsageSourceError::NotFound(message)
            if message.contains("ada-lovelace")),
        "{missing_user}"
    );
    let missing_org = source.summary(october()).await.unwrap_err();
    assert!(
        matches!(&missing_org, MeteredUsageSourceError::NotFound(message)
            if message.contains("not installed")),
        "{missing_org}"
    );

    github.answer_with(
        ORG_MONTH_PATH,
        StatusCode::TOO_MANY_REQUESTS,
        vec![("retry-after", "30".to_string())],
        "{}",
    );
    assert!(matches!(
        source
            .credits(AiMeteredScope::Organization, AiMeteredPeriod::Month(october()))
            .await,
        Err(MeteredUsageSourceError::RateLimited { retry_after: Some(wait) })
            if wait == Duration::from_secs(30)
    ));
    github.answer_with(
        USAGE_MONTH_PATH,
        StatusCode::UNPROCESSABLE_ENTITY,
        Vec::new(),
        r#"{"message": "bad month"}"#,
    );
    assert!(matches!(
        source.org_usage(AiMeteredPeriod::Month(october())).await,
        Err(MeteredUsageSourceError::Response(_))
    ));
    github.answer_with(ORG_DAY_PATH, StatusCode::BAD_GATEWAY, Vec::new(), "{}");
    assert!(matches!(
        source
            .credits(
                AiMeteredScope::Organization,
                AiMeteredPeriod::Day(date!(2026 - 10 - 05))
            )
            .await,
        Err(MeteredUsageSourceError::Unavailable(_))
    ));
}

#[tokio::test]
async fn one_app_mints_once_for_seats_and_usage() {
    let (api_base, github) = fake_github().await;
    github.answer(SUMMARY_PATH, SUMMARY);
    let shared = app(&api_base);
    let directory = GithubAccountDirectory::new(shared.clone());
    let usage = GithubMeteredUsageSource::new(shared);

    directory
        .profile(AiProviderAccountId::new(1001).unwrap())
        .await
        .unwrap();
    usage.summary(october()).await.unwrap();
    assert_eq!(github.mints(), 1);
    assert_eq!(github.paths(), ["/user/1001", SUMMARY_PATH]);
}

#[tokio::test]
async fn requests_start_at_most_ten_a_second() {
    let (api_base, github) = fake_github().await;
    github.answer(SUMMARY_PATH, SUMMARY);
    let source = source(&api_base);

    // Concurrent callers must queue: a slow server cannot space them out.
    let requests = (0..12).map(|_| source.summary(october()));
    for result in futures::future::join_all(requests).await {
        result.unwrap();
    }

    let mut arrivals: Vec<_> = github
        .received()
        .into_iter()
        .map(|received| received.arrived)
        .collect();
    arrivals.sort();
    assert_eq!(arrivals.len(), 12);
    // Any eleven consecutive arrivals span a second, less transport jitter.
    for window in arrivals.windows(11) {
        let span = window[10] - window[0];
        assert!(span >= Duration::from_millis(950), "{span:?}");
    }
}

/// Checks the parsing against GitHub responses saved in a local directory,
/// never committed (`<login>_d<day>.json`, `<login>_month.json`,
/// `org_d<day>.json`, `org_month.json`, `org_usage_d<day>.json`). Prints
/// booleans and counts only.
#[test]
#[ignore = "needs TOKI_METERED_ORACLE_DIR with saved GitHub responses"]
fn metered_import_reproduces_the_local_raw_files() {
    let Ok(directory) = std::env::var("TOKI_METERED_ORACLE_DIR") else {
        panic!("set TOKI_METERED_ORACLE_DIR");
    };
    let mut files: Vec<(String, String)> = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_stem()?.to_str()?.to_string();
            (path.extension()? == "json").then(|| (name, std::fs::read_to_string(&path).unwrap()))
        })
        .collect();
    files.sort();

    /// The period and organization a body names.
    fn period_of(body: &str) -> (AiMeteredPeriod, String) {
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        let organization = value["organization"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let period = &value["timePeriod"];
        let year = i32::try_from(period["year"].as_i64().unwrap()).unwrap();
        let month = u8::try_from(period["month"].as_u64().unwrap()).unwrap();
        let month = AiBillingMonth::new(year, time::Month::try_from(month).unwrap()).unwrap();
        let period = match period["day"].as_u64() {
            Some(day) => AiMeteredPeriod::Day(
                month
                    .first_day()
                    .replace_day(u8::try_from(day).unwrap())
                    .unwrap(),
            ),
            None => AiMeteredPeriod::Month(month),
        };
        (period, organization)
    }

    let mut parsed_files = 0;
    let mut rows_checked = 0;
    let mut g_mismatches = 0;
    // login → day → rows; login → month rows.
    let mut days: BTreeMap<String, BTreeMap<u8, Vec<AiMeteredCredit>>> = BTreeMap::new();
    let mut months: BTreeMap<String, Vec<AiMeteredCredit>> = BTreeMap::new();
    let mut organization_name = String::new();
    for (name, body) in &files {
        if name.starts_with("org_usage_d") {
            let value: serde_json::Value = serde_json::from_str(body).unwrap();
            let first = &value["usageItems"][0];
            let organization = first["organizationName"].as_str().unwrap_or_default();
            let date = first["date"].as_str().unwrap();
            let day = utc_day(OffsetDateTime::parse(date, &Rfc3339).unwrap());
            parse_usage(body, organization, AiMeteredPeriod::Day(day)).unwrap();
            parsed_files += 1;
            continue;
        }
        let (period, organization) = period_of(body);
        organization_name = organization.clone();
        let (owner, part) = name.rsplit_once('_').unwrap();
        let rows = if owner == "org" {
            parse_credits(body, &organization, AiMeteredScope::Organization, period).unwrap()
        } else {
            let value: serde_json::Value = serde_json::from_str(body).unwrap();
            let login = AiProviderLogin::parse(value["user"].as_str().unwrap()).unwrap();
            parse_credits(body, &organization, AiMeteredScope::Account(&login), period).unwrap()
        };
        parsed_files += 1;

        // G against an independent float estimate of quantity × 10^7.
        let raw: serde_json::Value = serde_json::from_str(body).unwrap();
        let mut by_key: BTreeMap<(String, String), f64> = BTreeMap::new();
        for item in raw["usageItems"].as_array().unwrap() {
            *by_key
                .entry((
                    item["sku"].as_str().unwrap_or_default().to_string(),
                    item["model"].as_str().unwrap_or_default().to_string(),
                ))
                .or_default() += item["grossQuantity"].as_f64().unwrap();
        }
        for row in &rows {
            rows_checked += 1;
            let estimate =
                (by_key[&(row.sku.clone(), row.model.label.clone())] * 1e7).round() as i64;
            if (row.gross_usd().unwrap().nanos() - estimate).abs() > 1 {
                g_mismatches += 1;
            }
        }
        match (part, period) {
            ("month", _) => {
                months.insert(owner.to_string(), rows);
            }
            (_, AiMeteredPeriod::Day(day)) => {
                days.entry(owner.to_string())
                    .or_default()
                    .insert(day.day(), rows);
            }
            _ => panic!("unexpected file {name}"),
        }
    }
    println!("files parsed: {parsed_files}, rows checked: {rows_checked}");
    println!("rows whose G differs from quantity x 10^7: {g_mismatches}");
    assert_eq!(g_mismatches, 0);

    // Logins with a day file for every day through the latest day seen:
    // their days sum to their month, key by key.
    let last_day = days
        .values()
        .flat_map(|by_day| by_day.keys().copied())
        .max()
        .unwrap_or(0);
    let (mut complete, mut matching) = (0, 0);
    for (owner, by_day) in &days {
        if owner == "org" || (1..=last_day).any(|day| !by_day.contains_key(&day)) {
            continue;
        }
        let Some(month) = months.get(owner) else {
            continue;
        };
        complete += 1;
        let summed: Vec<AiMeteredCredit> = by_day.values().flatten().cloned().collect();
        if credit_totals_match(
            &credit_totals(month).unwrap(),
            &credit_totals(&summed).unwrap(),
        ) {
            matching += 1;
        }
    }
    println!("logins with every day through day {last_day}: {complete}, matching their month: {matching}");
    assert_eq!(complete, matching);

    // Per organization day, the residual against the logins with that day.
    if let Some(org_days) = days.get("org") {
        for (day, org_rows) in org_days {
            let org_total: i64 = org_rows
                .iter()
                .map(|row| row.gross_usd().unwrap().nanos())
                .sum();
            let logins: Vec<&Vec<AiMeteredCredit>> = days
                .iter()
                .filter(|(owner, _)| owner.as_str() != "org")
                .filter_map(|(_, by_day)| by_day.get(day))
                .collect();
            let login_total: i64 = logins
                .iter()
                .flat_map(|rows| rows.iter())
                .map(|row| row.gross_usd().unwrap().nanos())
                .sum();
            let residual = org_total - login_total;
            println!(
                "day {day}: logins with a file {}, residual zero {}, residual negative {}",
                logins.len(),
                residual.abs() <= 100,
                residual < -100
            );
        }
    }
    assert!(!organization_name.is_empty());
}
