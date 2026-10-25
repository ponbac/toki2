use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{
    AiAccountDispositionId, AiAccountDispositionTerms, AiAccountTarget, AiBillingUsage, AiProvider,
    AiProviderLogin, AiUsdNanos,
};

use AiSuggestionConfidence::{Probable, Verified};
use AiSuggestionEvidence as E;

fn noise() -> Vec<String> {
    namespace_noise("Example-AB")
}

fn user(id: i32, full_name: &str, email: &str) -> AiBillingDeveloper {
    AiBillingDeveloper {
        user_id: UserId::new(id),
        full_name: full_name.to_string(),
        email: email.to_string(),
    }
}

fn account(id: i64, login: &str, name: Option<&str>, email: Option<&str>) -> AiSuggestionAccount {
    AiSuggestionAccount {
        account_id: AiProviderAccountId::new(id).unwrap(),
        login: login.to_string(),
        name: name.map(str::to_string),
        email: email.map(str::to_string),
        default_period: Some(AiAccountPeriod::new(date!(2026 - 01 - 01), None).unwrap()),
    }
}

/// The invented staff every case suggests among.
fn staff() -> Vec<AiBillingDeveloper> {
    vec![
        user(1, "Ada Lovelace", "ada.lovelace@example.com"),
        user(2, "Alan Turing", "alan.turing@example.com"),
        user(3, "Grace Brewster Hopper", "grace.hopper@example.com"),
        user(4, "Kevin Hoang Dinh", "kevin.dinh@example.com"),
        user(5, "Erik Lindqvist", "erik.lindqvist@example.com"),
        user(6, "Erik Sjöberg", "erik.sjoberg@example.com"),
        user(7, "Hedy Lamarr", "hedy.lamarr@example.com"),
        user(8, "Patrik Johansson", "patrik.johansson@example.com"),
        user(9, "Åsa Ängström", "asa.angstrom@example.com"),
        user(10, "Oskar Nilsson", "oskar.nilsson@example.com"),
    ]
}

/// Suggestions for one account, as (user, confidence, evidence).
fn suggestions_for(
    target: AiSuggestionAccount,
    others: Vec<AiSuggestionAccount>,
    users: &[AiBillingDeveloper],
) -> Vec<(i32, AiSuggestionConfidence, Vec<AiSuggestionEvidence>)> {
    let id = target.account_id;
    let mut accounts = vec![target];
    accounts.extend(others);
    flatten(
        suggest_targets(&accounts, users, &HashMap::new(), &[], &noise())
            .remove(&id)
            .unwrap_or_default(),
    )
}

fn flatten(
    suggestions: Vec<AiAccountSuggestion>,
) -> Vec<(i32, AiSuggestionConfidence, Vec<AiSuggestionEvidence>)> {
    suggestions
        .into_iter()
        .map(|suggestion| {
            (
                suggestion.user_id.as_i32(),
                suggestion.confidence,
                suggestion.evidence,
            )
        })
        .collect()
}

fn alone(
    target: AiSuggestionAccount,
) -> Vec<(i32, AiSuggestionConfidence, Vec<AiSuggestionEvidence>)> {
    suggestions_for(target, vec![], &staff())
}

#[test]
fn names_fold_to_plain_lower_case_letters() {
    assert_eq!(fold_name("Åsa Ängström"), "asa angstrom");
    assert_eq!(fold_name("Bjørn Sæther"), "bjorn saether");
    assert_eq!(fold_name("Łukasz Straße"), "lukasz strasse");
    assert_eq!(fold_name("Œdipe Þór Đorđe"), "oedipe thor dorde");
    assert_eq!(
        name_tokens(" Anna-Karin  O'Neill "),
        ["anna", "karin", "o", "neill"]
    );
    assert_eq!(namespace_noise("Spinit-AB"), ["spinit"]);
}

#[test]
fn a_login_core_drops_digits_and_the_namespace() {
    let noise = namespace_noise("Spinit-AB");
    assert_eq!(login_core("hedy02spinit", &noise), "hedy");
    assert_eq!(login_core("spinit-hedy-lamarr", &noise), "hedylamarr");
    assert_eq!(login_core("HedyLamarrSpinit", &noise), "hedylamarr");
    assert_eq!(login_core("spinaturing", &noise), "spinaturing");
    assert_eq!(login_core("ada99", &noise), "ada");
    assert_eq!(login_core("Spinit", &noise), "");
}

#[test]
fn a_public_email_verifies_and_one_that_matches_nobody_suggests_nothing() {
    assert_eq!(
        alone(account(
            1001,
            "zz-top",
            None,
            Some(" Ada.Lovelace@Example.com ")
        )),
        [(1, Verified, vec![E::Email])]
    );
    assert!(alone(account(1002, "zz-top", None, Some("nobody@example.com"))).is_empty());
}

#[test]
fn a_full_name_verifies_in_any_order_and_folded() {
    assert_eq!(
        alone(account(1001, "zz-top", Some("Alan Turing"), None)),
        [(2, Verified, vec![E::Name])]
    );
    assert_eq!(
        alone(account(1002, "zz-top", Some("Ångström, Åsa"), None)),
        [(9, Verified, vec![E::Name])]
    );
}

#[test]
fn a_middle_name_on_either_side_is_only_probable() {
    // The profile drops the user's middle name.
    assert_eq!(
        alone(account(1001, "kd9530", Some("Kevin Dinh"), None)),
        [(4, Probable, vec![E::NameFirstLast])]
    );
    // The user has one the profile spells out.
    assert_eq!(
        alone(account(1002, "zz-top", Some("Grace B. Hopper"), None)),
        [(3, Probable, vec![E::NameFirstLast])]
    );
}

#[test]
fn a_last_name_initial_is_probable() {
    assert_eq!(
        alone(account(1001, "zz-top", Some("Alan T."), None)),
        [(2, Probable, vec![E::NameInitial])]
    );
}

#[test]
fn a_first_name_alone_counts_only_when_no_one_else_has_it() {
    assert_eq!(
        alone(account(1001, "zz-top", Some("Hedy"), None)),
        [(7, Probable, vec![E::NameFirstOnly])]
    );
    // Two Eriks: neither.
    assert!(alone(account(1002, "zz-top", Some("Erik"), None)).is_empty());
    assert_eq!(
        alone(account(1003, "oskar02example", None, None)),
        [(10, Probable, vec![E::LoginFirstOnly])]
    );
    assert!(alone(account(1004, "erik", None, None)).is_empty());
}

#[test]
fn logins_spelling_the_full_name_are_probable() {
    assert_eq!(
        alone(account(1001, "example-hedy-lamarr", None, None)),
        [(7, Probable, vec![E::LoginFullName])]
    );
    assert_eq!(
        alone(account(1002, "TuringAlan42", None, None)),
        [(2, Probable, vec![E::LoginFullName])]
    );
    // Login evidence never verifies, even next to a matching name.
    assert_eq!(
        alone(account(1003, "adalovelace", Some("Ada Lovelace"), None)),
        [(1, Verified, vec![E::Name, E::LoginFullName])]
    );
}

#[test]
fn logins_with_an_initial_or_a_surname_prefix_are_probable() {
    assert_eq!(
        alone(account(1001, "aturing", None, None)),
        [(2, Probable, vec![E::LoginInitialSurname])]
    );
    assert_eq!(
        alone(account(1002, "AlanTuEx", None, None)),
        [(2, Probable, vec![E::LoginFirstSurnamePrefix])]
    );
    // Too short to say.
    assert!(alone(account(1003, "aturi", None, None)).is_empty());
}

#[test]
fn a_login_with_someone_elses_surname_suggests_no_one() {
    // A Patrik exists, but not this one.
    assert!(alone(account(1001, "patrikbergman", None, None)).is_empty());
}

#[test]
fn two_users_with_the_same_name_are_both_only_probable() {
    let mut users = staff();
    users.push(user(11, "Ada Lovelace", "ada.lovelace2@example.com"));
    assert_eq!(
        suggestions_for(
            account(1001, "zz-top", Some("Ada Lovelace"), None),
            vec![],
            &users
        ),
        [(1, Probable, vec![E::Name]), (11, Probable, vec![E::Name]),]
    );
}

#[test]
fn one_user_verified_by_two_accounts_is_verified_by_neither() {
    assert_eq!(
        suggestions_for(
            account(1001, "zz-top", Some("Alan Turing"), None),
            vec![account(
                1002,
                "zz-bottom",
                None,
                Some("alan.turing@example.com")
            )],
            &staff(),
        ),
        [(2, Probable, vec![E::Name])]
    );
}

#[test]
fn users_already_linked_in_the_default_period_are_left_out() {
    let linked = AiAccountDisposition {
        id: AiAccountDispositionId::new(1),
        payer: AiProvider::Copilot,
        account_id: AiProviderAccountId::new(2002).unwrap(),
        login: AiProviderLogin::parse("alan-old").unwrap(),
        terms: AiAccountDispositionTerms {
            target: AiAccountTarget::User(UserId::new(2)),
            period: AiAccountPeriod::new(date!(2025 - 01 - 01), Some(date!(2026 - 01 - 31)))
                .unwrap(),
        },
        updated_at: datetime!(2026-02-01 08:00 UTC),
        updated_by: None,
    };
    let target = account(1001, "zz-top", Some("Alan Turing"), None);
    let found = suggest_targets(
        std::slice::from_ref(&target),
        &staff(),
        &HashMap::new(),
        std::slice::from_ref(&linked),
        &noise(),
    );
    assert!(found[&target.account_id].is_empty());

    // A link that ended before the default period does not count.
    let later = AiSuggestionAccount {
        default_period: Some(AiAccountPeriod::new(date!(2026 - 02 - 01), None).unwrap()),
        ..target
    };
    let found = suggest_targets(
        std::slice::from_ref(&later),
        &staff(),
        &HashMap::new(),
        &[linked],
        &noise(),
    );
    assert_eq!(found[&later.account_id].len(), 1);
}

#[test]
fn an_account_without_a_default_period_gets_no_suggestions() {
    let covered = AiSuggestionAccount {
        default_period: None,
        ..account(1001, "zz-top", Some("Alan Turing"), None)
    };
    let found = suggest_targets(&[covered], &staff(), &HashMap::new(), &[], &noise());
    assert!(found.is_empty());
}

#[test]
fn equally_good_candidates_rank_by_their_activity_at_the_payer() {
    let mut users = staff();
    users.push(user(11, "Ada Lovelace", "ada.lovelace2@example.com"));
    let active = |cents: i64, records: i64, days: u32| AiUserActivity {
        days,
        last_day: Some(date!(2026 - 10 - 20)),
        usage: AiBillingUsage {
            records,
            priced_cost: AiUsdNanos::new(cents * 10_000_000).unwrap(),
            ..AiBillingUsage::default()
        },
    };
    let target = account(1001, "zz-top", Some("Ada Lovelace"), None);
    let ranked = |activity: HashMap<UserId, AiUserActivity>| {
        flatten(
            suggest_targets(
                std::slice::from_ref(&target),
                &users,
                &activity,
                &[],
                &noise(),
            )
            .remove(&target.account_id)
            .unwrap(),
        )
        .into_iter()
        .map(|(user_id, _, _)| user_id)
        .collect::<Vec<_>>()
    };

    assert_eq!(ranked(HashMap::new()), [1, 11]);
    assert_eq!(
        ranked(HashMap::from([(UserId::new(11), active(500, 10, 2))])),
        [11, 1]
    );
    // Cost first, then records, then days.
    assert_eq!(
        ranked(HashMap::from([
            (UserId::new(1), active(500, 10, 9)),
            (UserId::new(11), active(500, 11, 1)),
        ])),
        [11, 1]
    );
}

#[test]
fn better_evidence_outranks_activity() {
    // Ada by name; Alan only by a login prefix but far more active.
    let mut activity = HashMap::new();
    activity.insert(
        UserId::new(2),
        AiUserActivity {
            days: 20,
            last_day: None,
            usage: AiBillingUsage {
                records: 1_000,
                priced_cost: AiUsdNanos::new(1_000_000_000_000).unwrap(),
                ..AiBillingUsage::default()
            },
        },
    );
    let target = account(1001, "alantuex", Some("Ada Lovelace"), None);
    let found = flatten(
        suggest_targets(
            std::slice::from_ref(&target),
            &staff(),
            &activity,
            &[],
            &noise(),
        )
        .remove(&target.account_id)
        .unwrap(),
    );
    assert_eq!(
        found,
        [
            (1, Verified, vec![E::Name]),
            (2, Probable, vec![E::LoginFirstSurnamePrefix]),
        ]
    );
}

/// Reproduces a local analyst's mapping of real accounts, which is never
/// committed. `TOKI_SUGGESTION_ORACLE_DIR` holds `users.tsv` (id, full name and
/// email, tab separated, exported read-only), `profiles/*.json` (GitHub
/// `GET /user/{id}` bodies) and `mapping.json` (`{ "<userId>": { "login",
/// "status": "verified" | "probable" } }`).
#[test]
#[ignore = "needs a local oracle"]
fn suggestions_reproduce_the_local_oracle() {
    let dir = std::env::var("TOKI_SUGGESTION_ORACLE_DIR").expect(
        "set TOKI_SUGGESTION_ORACLE_DIR to the oracle directory (see .docs/ai-usage-billing.md)",
    );
    let dir = std::path::Path::new(&dir);
    let users: Vec<AiBillingDeveloper> = std::fs::read_to_string(dir.join("users.tsv"))
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next().unwrap().trim().parse().unwrap();
            let full_name = fields.next().unwrap();
            let email = fields.next().unwrap_or("");
            user(id, full_name, email)
        })
        .collect();
    let mut accounts = Vec::new();
    let mut logins = HashMap::new();
    for entry in std::fs::read_dir(dir.join("profiles")).unwrap() {
        let profile: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap()).unwrap();
        let login = profile["login"].as_str().unwrap();
        let id = profile["id"].as_i64().unwrap();
        logins.insert(login.to_string(), AiProviderAccountId::new(id).unwrap());
        accounts.push(account(
            id,
            login,
            profile["name"].as_str(),
            profile["email"].as_str(),
        ));
    }
    let mapping: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("mapping.json")).unwrap()).unwrap();

    let found = suggest_targets(
        &accounts,
        &users,
        &HashMap::new(),
        &[],
        &namespace_noise("Spinit-AB"),
    );
    let tops = found
        .values()
        .map(|suggestions| suggestions.first().map(|top| top.confidence))
        .collect::<Vec<_>>();
    let count = |confidence| tops.iter().filter(|top| **top == confidence).count();
    eprintln!(
        "{} accounts: {} verified, {} probable, {} without a candidate",
        tops.len(),
        count(Some(Verified)),
        count(Some(Probable)),
        count(None)
    );

    let mapping = mapping.as_object().unwrap();
    assert!(!mapping.is_empty());
    for (user_id, expected) in mapping {
        let login = expected["login"].as_str().unwrap();
        let confidence = match expected["status"].as_str().unwrap() {
            "verified" => Verified,
            _ => Probable,
        };
        let suggestions = &found[&logins[login]];
        assert_eq!(
            suggestions
                .iter()
                .map(|suggestion| (suggestion.user_id.as_i32(), suggestion.confidence))
                .collect::<Vec<_>>(),
            [(user_id.parse::<i32>().unwrap(), confidence)],
            "{login}: {suggestions:?}"
        );
    }
}
