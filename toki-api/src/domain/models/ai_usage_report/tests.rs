use time::macros::{date, datetime};

use super::*;
use crate::domain::models::{AiStoredMapping, AiUsageTimeZone, TimeTrackingCompany};

const MACHINE_A: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

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

fn stored(key: &str, company_id: &str, project_id: &str, name: &str) -> AiStoredMapping {
    AiStoredMapping {
        project_key: key.to_string(),
        company: company(company_id),
        project: project(project_id, name),
    }
}

/// Mappings of four keys: two to Client A, one to Internal, one stale.
fn resolver(configured: bool) -> AiMappingResolver {
    AiMappingResolver::new(
        vec![
            stored("github.com/example/app", "company-1", "101", "Client A"),
            stored("Client A", "company-1", "101", "Client A (old name)"),
            stored("github.com/example/tools", "company-1", "202", "Internal"),
            stored("github.com/example/old", "company-2", "909", "Elsewhere"),
        ],
        configured.then(|| company("company-1")),
    )
}

fn totals(records: i64, cost: Option<f64>) -> AiUsageTotals {
    AiUsageTotals {
        tokens: AiTokenCounts {
            input: 10 * records,
            cache_read: 100 * records,
            cache_write: 5 * records,
            output: 7 * records,
        },
        records,
        priced_cost_usd: cost.unwrap_or(0.0),
        unpriced_buckets: i64::from(cost.is_none()),
        unpriced_records: if cost.is_none() { records } else { 0 },
    }
}

fn calendar() -> AiLocalCalendar {
    AiLocalCalendar {
        time_zone: AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
        today: date!(2026 - 09 - 22),
    }
}

fn september() -> AiUsageDateRange {
    AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), date!(2026 - 09 - 30)).unwrap()
}

#[test]
fn keys_are_attributed_through_the_mappings() {
    let resolution =
        |key: &str, configured| AiProjectAttribution::of(key.to_string(), &resolver(configured));

    assert_eq!(
        resolution("github.com/example/app", true).resolution,
        AiProjectResolution::Mapped(project("101", "Client A"))
    );
    assert_eq!(
        resolution("github.com/example/old", true).resolution,
        AiProjectResolution::Stale
    );
    assert_eq!(
        resolution("github.com/example/new", true).resolution,
        AiProjectResolution::Unmapped { mappable: true }
    );
    assert_eq!(
        resolution(" padded", true).resolution,
        AiProjectResolution::Unmapped { mappable: false }
    );
    assert_eq!(
        resolution(UNATTRIBUTED_PROJECT_KEY, true).resolution,
        AiProjectResolution::Unattributed
    );
    assert!(resolution("github.com/example/new", true).mappable());
    assert!(!resolution("github.com/example/app", true).mappable());

    // Without time tracking no mapping resolves, none is stale, and no key
    // can be mapped.
    assert_eq!(
        resolution("github.com/example/app", false).resolution,
        AiProjectResolution::Unconfigured
    );
    assert_eq!(
        resolution("github.com/example/old", false).resolution,
        AiProjectResolution::Unconfigured
    );
    assert!(!resolution("github.com/example/new", false).mappable());
}

#[test]
fn project_filters_select_the_keys_that_resolve() {
    let select = |project, configured| {
        AiUsageSelection::new(
            &AiUsageFilter {
                project,
                ..AiUsageFilter::default()
            },
            &resolver(configured),
        )
        .keys
    };
    let keys = |keys: &[&str]| keys.iter().map(ToString::to_string).collect::<Vec<_>>();

    assert_eq!(select(None, true), AiKeySelection::All);
    assert_eq!(
        select(Some(AiProjectFilter::Project(ProjectId::new("101"))), true),
        AiKeySelection::Only(keys(&["Client A", "github.com/example/app"]))
    );
    // Stale mappings count as unassigned, so they are not excluded.
    assert_eq!(
        select(Some(AiProjectFilter::Unassigned), true),
        AiKeySelection::Except(keys(&[
            "Client A",
            "github.com/example/app",
            "github.com/example/tools"
        ]))
    );
    assert_eq!(
        select(Some(AiProjectFilter::Project(ProjectId::new("101"))), false),
        AiKeySelection::Only(Vec::new())
    );
    assert_eq!(
        select(Some(AiProjectFilter::Unassigned), false),
        AiKeySelection::Except(Vec::new())
    );

    let selection = AiUsageSelection::new(&AiUsageFilter::default(), &resolver(true));
    assert_eq!(
        selection.projects,
        [
            ("Client A".to_string(), ProjectId::new("101")),
            ("github.com/example/app".to_string(), ProjectId::new("101")),
            (
                "github.com/example/tools".to_string(),
                ProjectId::new("202")
            ),
        ]
    );
}

#[test]
fn a_report_defaults_to_the_current_local_month() {
    let september = report_dates(date!(2026 - 09 - 22), None, None).unwrap();
    assert_eq!(
        (september.start(), september.last_day()),
        (date!(2026 - 09 - 01), date!(2026 - 09 - 30))
    );
    let leap_february = report_dates(date!(2028 - 02 - 01), None, None).unwrap();
    assert_eq!(leap_february.last_day(), date!(2028 - 02 - 29));

    // Only `to`: from the start of its month. Only `from`: to the end of this month.
    let to_only = report_dates(date!(2026 - 09 - 22), None, Some(date!(2026 - 08 - 10)));
    assert_eq!(to_only.unwrap().start(), date!(2026 - 08 - 01));
    let from_only = report_dates(date!(2026 - 09 - 22), Some(date!(2026 - 07 - 15)), None);
    assert_eq!(from_only.unwrap().last_day(), date!(2026 - 09 - 30));
}

#[test]
fn a_report_range_is_ordered_and_at_most_366_days() {
    let today = date!(2026 - 09 - 22);
    let invalid = |from, to| match report_dates(today, Some(from), Some(to)) {
        Err(AiUsageError::InvalidQuery(message)) => message,
        other => panic!("expected an invalid query, got {other:?}"),
    };

    assert!(invalid(date!(2026 - 09 - 02), date!(2026 - 09 - 01)).contains("after"));
    assert!(report_dates(
        today,
        Some(date!(2025 - 09 - 22)),
        Some(date!(2026 - 09 - 22))
    )
    .is_ok());
    assert!(invalid(date!(2025 - 09 - 21), date!(2026 - 09 - 22)).contains("366"));
}

#[test]
fn a_report_puts_keys_under_their_projects_and_unassigned_last_on_a_tie() {
    let key = |key: &str, records, cost| (key.to_string(), totals(records, cost));
    let report = AiUsageReport::assemble(
        calendar(),
        september(),
        &resolver(true),
        AiUsageOptionSums {
            providers: vec![AiProvider::Claude],
            models: vec![
                ("model-b".to_string(), totals(1, Some(0.5))),
                ("model-a".to_string(), totals(1, Some(2.0))),
            ],
            project_keys: vec![
                "github.com/example/app".to_string(),
                "Client A".to_string(),
                "github.com/example/old".to_string(),
            ],
            machines: vec![AiMachineId::parse(MACHINE_A).unwrap()],
        },
        AiUsageSums {
            projects: vec![
                // Level with Internal on cost and tokens: Unassigned goes last.
                (None, totals(1, Some(1.0))),
                (Some(ProjectId::new("101")), totals(3, Some(3.0))),
                (Some(ProjectId::new("202")), totals(1, Some(1.0))),
            ],
            keys: vec![
                key("github.com/example/app", 1, Some(1.0)),
                key("Client A", 2, Some(2.0)),
                key("github.com/example/tools", 1, Some(1.0)),
                key("github.com/example/old", 1, Some(0.5)),
                key(UNATTRIBUTED_PROJECT_KEY, 1, Some(0.5)),
            ],
            ..AiUsageSums::default()
        },
    );

    let groups = report
        .projects
        .iter()
        .map(|group| {
            (
                group.project.as_ref().map(|project| project.name.as_str()),
                group.totals.priced_cost_usd,
                group
                    .keys
                    .iter()
                    .map(|key| key.project.project_key.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        groups,
        [
            (
                Some("Client A"),
                3.0,
                vec!["Client A", "github.com/example/app"]
            ),
            (Some("Internal"), 1.0, vec!["github.com/example/tools"]),
            (
                None,
                1.0,
                vec!["github.com/example/old", UNATTRIBUTED_PROJECT_KEY]
            ),
        ]
    );
    assert!(report.time_tracking_configured);
    assert_eq!(report.options.models, ["model-a", "model-b"]);
    assert_eq!(report.options.projects, [project("101", "Client A")]);
    assert!(report.options.unassigned);
}

fn cursor(order: AiSessionOrder) -> AiSessionCursor {
    AiSessionCursor {
        order,
        rank: "12.345000".to_string(),
        last_hour: datetime!(2026-09-22 13:00 UTC),
        first_hour: datetime!(2026-09-22 08:00 UTC),
        machine_id: AiMachineId::parse(MACHINE_A).unwrap(),
        provider: AiProvider::Claude,
        session_key: "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f".to_string(),
    }
}

#[test]
fn session_cursors_round_trip_and_reject_anything_else() {
    let cost = cursor(AiSessionOrder::Cost);
    assert_eq!(
        AiSessionCursor::parse(&cost.encode(), AiSessionOrder::Cost).unwrap(),
        cost
    );
    assert!(
        AiSessionCursor::parse(&cost.encode(), AiSessionOrder::Recent)
            .unwrap_err()
            .to_string()
            .contains("another order")
    );

    let encoded = cost.encode();
    for invalid in [
        String::new(),
        "cost".to_string(),
        encoded.replace("12.345000", "12."),
        encoded.replace("12.345000", "-1"),
        encoded.replace("12.345000", "1e9"),
        encoded.replace("claude", "gemini"),
        encoded.replace("9b1f0c6e", "zz1f0c6e"),
        format!("{encoded}~extra"),
    ] {
        assert!(
            AiSessionCursor::parse(&invalid, AiSessionOrder::Cost).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn a_session_page_has_a_cursor_only_when_more_sessions_follow() {
    let row = |key: &str| AiUsageSessionRow {
        machine_id: AiMachineId::parse(MACHINE_A).unwrap(),
        provider: AiProvider::Claude,
        session_key: key.to_string(),
        project_keys: vec!["github.com/example/app".to_string()],
        models: vec!["model-a".to_string()],
        first_hour: datetime!(2026-09-22 08:00 UTC),
        last_hour: datetime!(2026-09-22 09:00 UTC),
        totals: totals(1, Some(0.5)),
        rank: "0.5".to_string(),
    };
    let page = |rows: Vec<AiUsageSessionRow>| {
        AiUsageSessionList::page(
            calendar(),
            september(),
            AiSessionOrder::Cost,
            2,
            AiUsageSessionRows { total: 3, rows },
            &resolver(true),
        )
    };

    let first = page(vec![row("aa"), row("bb"), row("cc")]);
    assert_eq!(first.sessions.len(), 2);
    assert_eq!(
        first.next.map(|cursor| cursor.session_key),
        Some("bb".to_string())
    );
    assert_eq!(
        first.sessions[0].projects[0].project(),
        Some(&project("101", "Client A"))
    );

    let last = page(vec![row("cc")]);
    assert_eq!((last.sessions.len(), last.next), (1, None));
}

#[test]
fn a_session_listing_returns_1_to_1000_sessions() {
    assert_eq!(sessions_limit(None).unwrap(), DEFAULT_SESSIONS_LIMIT);
    assert_eq!(sessions_limit(Some(1_000)).unwrap(), 1_000);
    assert!(sessions_limit(Some(0)).is_err());
    assert!(sessions_limit(Some(1_001)).is_err());
}

#[test]
fn a_machine_is_stale_after_seven_days_without_a_sync() {
    let status = AiMachineStatus {
        machine: AiMachine {
            id: AiMachineId::parse(MACHINE_A).unwrap(),
            label: "laptop".to_string(),
            client_version: "0.2.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        first_seen_at: datetime!(2026-09-01 08:00 UTC),
        last_synced_at: datetime!(2026-09-14 08:00 UTC),
        coverage: Vec::new(),
    };

    assert!(!status.is_stale_at(datetime!(2026-09-21 08:00 UTC)));
    assert!(status.is_stale_at(datetime!(2026-09-21 08:00:01 UTC)));
}
