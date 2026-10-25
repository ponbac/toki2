use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{AiStoredMapping, ProjectId, UserId};

fn company(id: &str) -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: id.to_string(),
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
    project("202", "Client B")
}

fn stored(
    repository: &str,
    company_id: &str,
    project: AiMappedProject,
) -> AiStoredRepositoryMapping {
    AiStoredRepositoryMapping {
        repository: repository.to_string(),
        company: company(company_id),
        project,
    }
}

fn key_mapping(key: &str, company_id: &str, project: AiMappedProject) -> AiStoredMapping {
    AiStoredMapping {
        project_key: key.to_string(),
        company: company(company_id),
        project,
    }
}

fn credits(repository: &str, day: Date, credits: i64) -> AiRepositoryDayCredits {
    AiRepositoryDayCredits {
        repository: repository.to_string(),
        day,
        credits: AiUsdNanos::new(credits * 10_000_000).unwrap(),
    }
}

fn mapping(repository: &str, company_id: &str, project: AiMappedProject) -> AiRepositoryMapping {
    AiRepositoryMapping {
        payer: AiProvider::Copilot,
        repository: repository.to_string(),
        company: company(company_id),
        project,
        created_by: Some(AiProjectMappingEditor {
            id: UserId::new(1),
            full_name: "Ada Lovelace".to_string(),
        }),
        created_at: datetime!(2026-10-20 08:00 UTC),
        updated_by: None,
        updated_at: datetime!(2026-10-20 08:00 UTC),
    }
}

fn month(raw: &str) -> AiBillingMonth {
    AiBillingMonth::parse(raw).unwrap()
}

#[test]
fn repository_names_are_one_to_two_hundred_characters_kept_as_given() {
    for valid in [
        "widget-api",
        "Widget-API",
        " padded ",
        "é",
        &"a".repeat(200),
    ] {
        let name = AiRepositoryName::parse(valid).unwrap();
        assert_eq!(name.as_str(), valid);
        assert_eq!(name.key(), valid.to_lowercase());
    }
    for invalid in [String::new(), "a".repeat(201), "widget\0api".to_string()] {
        assert!(
            matches!(
                AiRepositoryName::parse(&invalid),
                Err(AiRepositoryMappingError::InvalidRepository(_))
            ),
            "{invalid:?}"
        );
    }
}

#[test]
fn the_resolver_matches_repositories_case_insensitively_against_the_configured_company() {
    let mappings = vec![
        stored("widget-api", "company-1", client_a()),
        stored("Gadget-Web", "company-2", client_b()),
    ];
    let resolver = AiRepositoryResolver::new(mappings.clone(), Some(company("company-1")));
    for spelling in ["widget-api", "Widget-API", "WIDGET-API"] {
        assert_eq!(resolver.project(spelling), Some(&client_a()), "{spelling}");
    }
    assert_eq!(
        resolver
            .mapping("gadget-web")
            .map(|(_, resolution)| resolution),
        Some(AiMappingResolution::Stale)
    );
    assert_eq!(resolver.project("gadget-web"), None);
    assert_eq!(resolver.mapping("other-repo"), None);

    let unconfigured = AiRepositoryResolver::new(mappings, None);
    assert_eq!(
        unconfigured
            .mapping("widget-api")
            .map(|(_, resolution)| resolution),
        Some(AiMappingResolution::Unconfigured)
    );
    assert_eq!(unconfigured.project("widget-api"), None);
    assert_eq!(AiRepositoryResolver::default().project("widget-api"), None);
}

#[test]
fn suggestions_come_from_resolving_mapped_keys_naming_the_repository() {
    let keys = AiMappingResolver::new(
        vec![
            key_mapping("github.com/example-org/widget-api", "company-1", client_b()),
            // Any owner, host and name in any case.
            key_mapping("GitHub.com/other-owner/Widget-API", "company-1", client_a()),
            key_mapping("github.com/third-owner/widget-api", "company-1", client_a()),
            // Not `{host}/{owner}/{name}`, another host, an empty owner or a
            // longer path: never a suggestion.
            key_mapping(
                "example-org/widget-api",
                "company-1",
                project("303", "Other"),
            ),
            key_mapping(
                "gitlab.com/example-org/widget-api",
                "company-1",
                project("303", "Other"),
            ),
            key_mapping(
                "github.com//widget-api",
                "company-1",
                project("303", "Other"),
            ),
            key_mapping(
                "github.com/example-org/widget-api/docs",
                "company-1",
                project("303", "Other"),
            ),
            key_mapping(
                "github.com/example-org/widget-api-v2",
                "company-1",
                project("303", "Other"),
            ),
            // A stale mapping resolves to nothing.
            key_mapping(
                "github.com/stale-owner/widget-api",
                "company-2",
                project("404", "Stale"),
            ),
        ],
        Some(company("company-1")),
    );
    let suggestions = suggest_repository_projects("Widget-Api", "github.com", &keys);
    assert_eq!(
        suggestions,
        [
            // Two keys first, then one.
            AiRepositorySuggestion {
                project: client_a(),
                project_keys: vec![
                    "GitHub.com/other-owner/Widget-API".to_string(),
                    "github.com/third-owner/widget-api".to_string(),
                ],
            },
            AiRepositorySuggestion {
                project: client_b(),
                project_keys: vec!["github.com/example-org/widget-api".to_string()],
            },
        ]
    );
    assert!(suggest_repository_projects("gadget-web", "github.com", &keys).is_empty());

    // Equally many keys: by project name, then id.
    let tied = AiMappingResolver::new(
        vec![
            key_mapping("github.com/b/widget-api", "company-1", client_b()),
            key_mapping("github.com/a/widget-api", "company-1", client_a()),
        ],
        Some(company("company-1")),
    );
    let names: Vec<String> = suggest_repository_projects("widget-api", "github.com", &tied)
        .into_iter()
        .map(|suggestion| suggestion.project.name)
        .collect();
    assert_eq!(names, ["Client A", "Client B"]);

    // Without a configured company no mapping resolves, so none suggests.
    let unconfigured = AiMappingResolver::new(
        vec![key_mapping(
            "github.com/a/widget-api",
            "company-1",
            client_a(),
        )],
        None,
    );
    assert!(suggest_repository_projects("widget-api", "github.com", &unconfigured).is_empty());
}

#[test]
fn the_billed_spelling_is_the_latest_days() {
    let rows = vec![
        credits("Widget-API", date!(2026 - 10 - 01), 1),
        credits("widget-api", date!(2026 - 10 - 20), 1),
        credits("WIDGET-API", date!(2026 - 10 - 05), 1),
        credits("gadget-web", date!(2026 - 10 - 25), 1),
    ];
    assert_eq!(
        billed_spelling(&rows, "widget-api").as_deref(),
        Some("widget-api")
    );
    assert_eq!(billed_spelling(&rows, "other"), None);
    // On one day, the greater string, as the bill picks it.
    let same_day = vec![
        credits("Widget-API", date!(2026 - 10 - 20), 1),
        credits("widget-api", date!(2026 - 10 - 20), 1),
    ];
    assert_eq!(
        billed_spelling(&same_day, "widget-api").as_deref(),
        Some("widget-api")
    );
}

/// A row as `(repository, credits per month, last billed, resolution,
/// suggestions)`.
type RowSummary<'a> = (
    &'a str,
    Vec<i64>,
    Option<Date>,
    Option<AiMappingResolution>,
    usize,
);

#[test]
fn the_overview_lists_billed_and_mapped_repositories_unmapped_ones_first() {
    let keys = AiMappingResolver::new(
        vec![key_mapping(
            "github.com/example-org/gadget-web",
            "company-1",
            client_b(),
        )],
        Some(company("company-1")),
    );
    let configured = company("company-1");
    let overview = repository_overview(AiRepositoryOverviewInput {
        payer: AiProvider::Copilot,
        months: vec![month("2026-09"), month("2026-10")],
        billed_from: Some(month("2026-10")),
        credits: vec![
            credits("widget-api", date!(2026 - 09 - 30), 100),
            credits("Widget-API", date!(2026 - 10 - 01), 200),
            credits("widget-api", date!(2026 - 10 - 24), 1_000),
            credits("gadget-web", date!(2026 - 10 - 20), 800),
            // Before the shown months: listed, without credits shown.
            credits("legacy-tool", date!(2026 - 08 - 31), 50),
        ],
        mappings: vec![
            mapping("WIDGET-API", "company-1", client_a()),
            mapping("archived-repo", "company-2", client_b()),
        ],
        configured: Some(&configured),
        keys: &keys,
        key_host: "github.com",
    });
    assert_eq!(overview.payer, AiProvider::Copilot);
    assert_eq!(overview.billed_from, Some(month("2026-10")));
    assert_eq!(overview.months, [month("2026-09"), month("2026-10")]);
    let rows: Vec<RowSummary<'_>> = overview
        .repositories
        .iter()
        .map(|row| {
            (
                row.repository.as_str(),
                row.months
                    .iter()
                    .map(|month| month.credits.nanos() / 10_000_000)
                    .collect(),
                row.last_billed_on,
                row.mapping.as_ref().map(|entry| entry.resolution),
                row.suggestions.len(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            // Credits without a resolving mapping: a look first.
            (
                "gadget-web",
                vec![0, 800],
                Some(date!(2026 - 10 - 20)),
                None,
                1
            ),
            // A stale mapping of a repository never billed.
            (
                "archived-repo",
                vec![0, 0],
                None,
                Some(AiMappingResolution::Stale),
                0
            ),
            (
                "legacy-tool",
                vec![0, 0],
                Some(date!(2026 - 08 - 31)),
                None,
                0
            ),
            // Its latest spelling, months by UTC day, and no suggestions:
            // its mapping resolves.
            (
                "widget-api",
                vec![100, 1_200],
                Some(date!(2026 - 10 - 24)),
                Some(AiMappingResolution::Resolves),
                0
            ),
        ]
    );
    let widget = &overview.repositories[3];
    assert_eq!(
        widget.mapping.as_ref().unwrap().mapping.repository,
        "WIDGET-API"
    );
    assert_eq!(
        overview.repositories[0].suggestions[0].project_keys,
        ["github.com/example-org/gadget-web"]
    );
}

#[test]
fn a_month_bills_from_the_cutover_once_it_rounds_to_a_credit() {
    let october = month("2026-10");
    let nanos = |nanos: i64| AiUsdNanos::new(nanos).unwrap();
    let bills = |month: AiBillingMonth, credits: i64, billed_from: Option<AiBillingMonth>| {
        AiRepositoryMonthCredits::new(month, nanos(credits), billed_from).billed
    };
    // Half a credit rounds up to one; just under it rounds to none.
    assert!(bills(october, 5_000_000, Some(october)));
    assert!(!bills(october, 4_999_999, Some(october)));
    assert!(!bills(october, 0, Some(october)));
    // After the cutover month too, never before it or without one.
    assert!(bills(month("2026-11"), 10_000_000, Some(october)));
    assert!(!bills(month("2026-09"), 10_000_000, Some(october)));
    assert!(!bills(october, 10_000_000, None));
}

#[test]
fn only_credits_that_bill_a_line_need_a_mapping() {
    let keys = AiMappingResolver::new(Vec::new(), Some(company("company-1")));
    let configured = company("company-1");
    let overview = |billed_from: Option<AiBillingMonth>| {
        repository_overview(AiRepositoryOverviewInput {
            payer: AiProvider::Copilot,
            months: vec![month("2026-09"), month("2026-10")],
            billed_from,
            credits: vec![
                // Only before the cutover: the bill has no line for it.
                credits("before-cutover", date!(2026 - 09 - 30), 500),
                // 0.4 credits in October, across two days: rounds to none.
                AiRepositoryDayCredits {
                    repository: "under-a-credit".to_string(),
                    day: date!(2026 - 10 - 02),
                    credits: AiUsdNanos::new(2_000_000).unwrap(),
                },
                AiRepositoryDayCredits {
                    repository: "under-a-credit".to_string(),
                    day: date!(2026 - 10 - 03),
                    credits: AiUsdNanos::new(2_000_000).unwrap(),
                },
                // Two half credits round to one once summed for the month.
                AiRepositoryDayCredits {
                    repository: "widget-api".to_string(),
                    day: date!(2026 - 10 - 02),
                    credits: AiUsdNanos::new(3_000_000).unwrap(),
                },
                AiRepositoryDayCredits {
                    repository: "widget-api".to_string(),
                    day: date!(2026 - 10 - 03),
                    credits: AiUsdNanos::new(2_000_000).unwrap(),
                },
            ],
            mappings: Vec::new(),
            configured: Some(&configured),
            keys: &keys,
            key_host: "github.com",
        })
    };
    let summary = |overview: &AiRepositoryMappingOverview| -> Vec<(String, Vec<bool>, bool)> {
        overview
            .repositories
            .iter()
            .map(|row| {
                (
                    row.repository.clone(),
                    row.months.iter().map(|month| month.billed).collect(),
                    row.needs_mapping(),
                )
            })
            .collect()
    };
    assert_eq!(
        summary(&overview(Some(month("2026-10")))),
        [
            ("widget-api".to_string(), vec![false, true], true),
            ("before-cutover".to_string(), vec![false, false], false),
            ("under-a-credit".to_string(), vec![false, false], false),
        ]
    );
    // Without a cutover no month bills, so nothing needs a mapping yet.
    assert_eq!(
        summary(&overview(None)),
        [
            ("before-cutover".to_string(), vec![false, false], false),
            ("under-a-credit".to_string(), vec![false, false], false),
            ("widget-api".to_string(), vec![false, false], false),
        ]
    );
}
