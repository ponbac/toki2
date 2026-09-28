use time::macros::datetime;

use super::*;

const SESSION: &str = "9b1f0c6e2d4a8b7c3e5f1a2b4c6d8e0f";

fn window() -> AiUsageWindow {
    AiUsageWindow::new(
        datetime!(2026-09-21 22:00 UTC),
        datetime!(2026-09-22 22:00 UTC),
    )
    .unwrap()
}

fn bucket() -> AiUsageBucket {
    AiUsageBucket {
        hour_start: datetime!(2026-09-22 08:00 UTC),
        session_key: SESSION.to_string(),
        project_key: "github.com/example/app".to_string(),
        provider: AiProvider::Claude,
        model: "example-model".to_string(),
        tokens: AiTokenCounts {
            input: 1_200,
            cache_read: 48_000,
            cache_write: 3_000,
            output: 900,
        },
        records: 14,
        estimated_cost_usd: Some(0.0421),
        unpriced_records: 0,
    }
}

fn upload(buckets: Vec<AiUsageBucket>) -> Result<AiUsageUpload, AiUsageError> {
    upload_with_hints(buckets, Vec::new())
}

fn upload_with_hints(
    buckets: Vec<AiUsageBucket>,
    hints: Vec<AiProviderHint>,
) -> Result<AiUsageUpload, AiUsageError> {
    upload_covering(
        &[
            (AiProvider::Claude, AiCoverageStatus::Ok),
            (AiProvider::Codex, AiCoverageStatus::Ok),
        ],
        buckets,
        hints,
    )
}

fn upload_covering(
    coverage: &[(AiProvider, AiCoverageStatus)],
    buckets: Vec<AiUsageBucket>,
    hints: Vec<AiProviderHint>,
) -> Result<AiUsageUpload, AiUsageError> {
    AiUsageUpload::new(
        AiMachine {
            id: AiMachineId::parse("5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a").unwrap(),
            label: "work-laptop".to_string(),
            client_version: "0.2.0".to_string(),
            time_zone: "Europe/Stockholm".to_string(),
        },
        window(),
        AiPricing {
            status: AiPricingStatus::Fresh,
            fetched_at: None,
            source: "https://example.com/prices.json".to_string(),
        },
        buckets,
        coverage
            .iter()
            .map(|&(provider, status)| AiProviderCoverage {
                provider,
                status,
                files: 1,
                unreadable: 0,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 0,
            })
            .collect(),
        hints,
    )
}

fn message(result: Result<AiUsageUpload, AiUsageError>) -> String {
    match result {
        Err(AiUsageError::InvalidUpload(message)) => message,
        other => panic!("expected an invalid upload, got {other:?}"),
    }
}

fn rejection(buckets: Vec<AiUsageBucket>) -> String {
    message(upload(buckets))
}

#[test]
fn window_is_whole_utc_hours_with_start_before_end() {
    assert!(AiUsageWindow::new(
        datetime!(2026-09-22 08:00 +02:00),
        datetime!(2026-09-22 09:00 +02:00)
    )
    .is_ok());
    assert!(AiUsageWindow::new(
        datetime!(2026-09-22 08:30 UTC),
        datetime!(2026-09-22 10:00 UTC)
    )
    .is_err());
    // A whole hour in a half-hour zone is not a whole UTC hour.
    assert!(AiUsageWindow::new(
        datetime!(2026-09-22 08:00 +05:30),
        datetime!(2026-09-22 12:30 UTC)
    )
    .is_err());
    assert!(AiUsageWindow::new(
        datetime!(2026-09-22 10:00 UTC),
        datetime!(2026-09-22 10:00 UTC)
    )
    .is_err());
}

#[test]
fn window_is_half_open() {
    let window = window();
    assert!(window.contains(window.start()));
    assert!(!window.contains(window.end()));
}

#[test]
fn accepts_a_valid_upload_and_normalises_hours_to_utc() {
    let mut offset = bucket();
    offset.hour_start = datetime!(2026-09-22 11:00 +02:00);
    offset.estimated_cost_usd = None;
    offset.unpriced_records = 3;
    offset.model = String::new();

    let upload = upload(vec![bucket(), offset]).unwrap();
    assert_eq!(upload.buckets().len(), 2);
    assert_eq!(upload.buckets()[1].hour_start.offset(), UtcOffset::UTC);
}

#[test]
fn rejects_invalid_buckets() {
    type Mutation = fn(&mut AiUsageBucket);
    let cases: Vec<(&str, Mutation)> = vec![
        ("inside the window", |b| {
            b.hour_start = datetime!(2026-09-22 22:00 UTC)
        }),
        ("inside the window", |b| {
            b.hour_start = datetime!(2026-09-21 21:00 UTC)
        }),
        ("whole UTC hour", |b| {
            b.hour_start = datetime!(2026-09-22 08:30 UTC)
        }),
        ("sessionKey", |b| b.session_key = SESSION.to_uppercase()),
        ("sessionKey", |b| b.session_key = SESSION[..31].to_string()),
        ("project", |b| b.project_key = String::new()),
        ("NUL", |b| b.model = "example\0model".to_string()),
        ("coverage as ok or partial", |b| {
            b.provider = AiProvider::Grok
        }),
        ("token counts", |b| b.tokens.cache_read = -1),
        ("records", |b| b.records = 0),
        ("unpricedRecords", |b| b.unpriced_records = -1),
        ("estimatedCostUsd", |b| b.estimated_cost_usd = Some(-0.01)),
        ("estimatedCostUsd", |b| {
            b.estimated_cost_usd = Some(f64::NAN)
        }),
        ("must not exceed records", |b| {
            b.estimated_cost_usd = None;
            b.unpriced_records = b.records + 1;
        }),
        ("exactly when", |b| {
            b.estimated_cost_usd = None;
            b.unpriced_records = 0;
        }),
        ("exactly when", |b| b.unpriced_records = 1),
        ("512", |b| b.project_key = "p".repeat(513)),
        ("512", |b| b.model = "m".repeat(513)),
    ];

    for (expected, mutate) in cases {
        let mut invalid = bucket();
        mutate(&mut invalid);
        let message = rejection(vec![bucket_at(0), invalid]);
        assert!(
            message.starts_with("buckets[1]: ") && message.contains(expected),
            "expected {expected:?}, got {message:?}"
        );
    }
}

#[test]
fn rejects_a_repeated_bucket_key() {
    let mut other_model = bucket();
    other_model.model = "other-model".to_string();
    assert!(upload(vec![bucket(), other_model]).is_ok());

    let mut same_key = bucket();
    same_key.records = 1;
    assert!(rejection(vec![bucket(), same_key]).contains("duplicate key"));
}

#[test]
fn uploaded_counts_follow_the_clients_safe_integer_contract() {
    const MAX_SAFE: i64 = 9_007_199_254_740_991;
    let mut boundary = bucket();
    boundary.tokens = AiTokenCounts {
        input: MAX_SAFE,
        cache_read: MAX_SAFE,
        cache_write: MAX_SAFE,
        output: MAX_SAFE,
    };
    boundary.records = MAX_SAFE;
    boundary.unpriced_records = MAX_SAFE;
    boundary.estimated_cost_usd = None;
    assert!(upload(vec![boundary.clone()]).is_ok());

    type BucketCount = fn(&mut AiUsageBucket) -> &mut i64;
    let bucket_counts: [BucketCount; 6] = [
        |bucket| &mut bucket.tokens.input,
        |bucket| &mut bucket.tokens.cache_read,
        |bucket| &mut bucket.tokens.cache_write,
        |bucket| &mut bucket.tokens.output,
        |bucket| &mut bucket.records,
        |bucket| &mut bucket.unpriced_records,
    ];
    for count in bucket_counts {
        let mut invalid = boundary.clone();
        *count(&mut invalid) = MAX_SAFE + 1;
        assert!(rejection(vec![invalid]).contains("safe integers"));
    }

    let original = upload(Vec::new()).unwrap();
    let boundary = AiProviderCoverage {
        provider: AiProvider::Claude,
        status: AiCoverageStatus::Failed,
        files: MAX_SAFE,
        unreadable: MAX_SAFE,
        malformed_lines: MAX_SAFE,
        skipped_records: MAX_SAFE,
        duplicates: MAX_SAFE,
    };
    let with_coverage = |coverage| {
        AiUsageUpload::new(
            original.machine().clone(),
            *original.window(),
            original.pricing().clone(),
            Vec::new(),
            vec![coverage],
            Vec::new(),
        )
    };
    assert!(with_coverage(boundary.clone()).is_ok());

    type CoverageCount = fn(&mut AiProviderCoverage) -> &mut i64;
    let coverage_counts: [CoverageCount; 5] = [
        |coverage| &mut coverage.files,
        |coverage| &mut coverage.unreadable,
        |coverage| &mut coverage.malformed_lines,
        |coverage| &mut coverage.skipped_records,
        |coverage| &mut coverage.duplicates,
    ];
    for count in coverage_counts {
        let mut invalid = boundary.clone();
        *count(&mut invalid) = MAX_SAFE + 1;
        assert!(message(with_coverage(invalid)).contains("safe integers"));
    }
}

#[test]
fn provider_hints_are_distinct_and_lie_inside_the_window() {
    let hint = AiProviderHint {
        provider: AiProvider::Codex,
        hour_start: datetime!(2026-09-22 08:00 UTC),
        plan: "pro".to_string(),
    };
    let other_plan = AiProviderHint {
        plan: "plus".to_string(),
        ..hint.clone()
    };
    let upload = upload_with_hints(Vec::new(), vec![hint.clone(), other_plan.clone()]).unwrap();
    assert_eq!(upload.provider_hints(), [hint.clone(), other_plan]);

    for (invalid, expected) in [
        (
            AiProviderHint {
                hour_start: datetime!(2026-09-22 22:00 UTC),
                ..hint.clone()
            },
            "inside the window",
        ),
        (
            AiProviderHint {
                hour_start: datetime!(2026-09-22 08:30 UTC),
                ..hint.clone()
            },
            "whole UTC hour",
        ),
        (
            AiProviderHint {
                plan: "p".repeat(513),
                ..hint.clone()
            },
            "512",
        ),
        (hint.clone(), "duplicate"),
    ] {
        let message = message(upload_with_hints(Vec::new(), vec![hint.clone(), invalid]));
        assert!(
            message.starts_with("providerHints[1]: ") && message.contains(expected),
            "expected {expected:?}, got {message:?}"
        );
    }
}

#[test]
fn machine_and_pricing_text_is_at_most_512_characters() {
    let long = |length: usize| "é".repeat(length);
    let with_machine = |label: String, source: String| {
        AiUsageUpload::new(
            AiMachine {
                id: AiMachineId::parse("5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a").unwrap(),
                label,
                client_version: "0.2.0".to_string(),
                time_zone: "Europe/Stockholm".to_string(),
            },
            window(),
            AiPricing {
                status: AiPricingStatus::Custom,
                fetched_at: None,
                source,
            },
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    };

    assert!(with_machine(long(512), long(512)).is_ok());
    assert_eq!(
        message(with_machine(long(513), String::new())),
        "machine.label must be at most 512 characters"
    );
    assert_eq!(
        message(with_machine("laptop".to_string(), long(513))),
        "pricing.source must be at most 512 characters"
    );
}

#[test]
fn only_ok_and_partial_coverage_replaces_a_provider() {
    let coverage = [
        (AiProvider::Claude, AiCoverageStatus::Ok),
        (AiProvider::Codex, AiCoverageStatus::Partial),
        (AiProvider::Grok, AiCoverageStatus::Missing),
        (AiProvider::Copilot, AiCoverageStatus::Failed),
    ];
    let mut partial = bucket();
    partial.provider = AiProvider::Codex;
    let upload = upload_covering(&coverage, vec![bucket(), partial], Vec::new()).unwrap();
    assert_eq!(
        upload.replaced_providers(),
        [AiProvider::Claude, AiProvider::Codex]
    );

    for provider in [AiProvider::Grok, AiProvider::Copilot] {
        let mut unreplaced = bucket();
        unreplaced.provider = provider;
        assert_eq!(
            message(upload_covering(&coverage, vec![unreplaced], Vec::new())),
            "buckets[0]: provider must be listed in coverage as ok or partial"
        );
        let hint = AiProviderHint {
            provider,
            hour_start: datetime!(2026-09-22 08:00 UTC),
            plan: "pro".to_string(),
        };
        assert_eq!(
            message(upload_covering(&coverage, Vec::new(), vec![hint])),
            "providerHints[0]: provider must be listed in coverage as ok or partial"
        );
    }

    let repeated = [
        (AiProvider::Claude, AiCoverageStatus::Ok),
        (AiProvider::Claude, AiCoverageStatus::Missing),
    ];
    assert!(message(upload_covering(&repeated, Vec::new(), Vec::new()))
        .contains("listed more than once"));
}

#[test]
fn replacing_coverage_requires_all_existing_history_to_be_read() {
    let original = upload(Vec::new()).unwrap();
    let with_coverage = |status, unreadable| {
        AiUsageUpload::new(
            original.machine().clone(),
            *original.window(),
            original.pricing().clone(),
            Vec::new(),
            vec![AiProviderCoverage {
                provider: AiProvider::Claude,
                status,
                files: 1,
                unreadable,
                malformed_lines: 0,
                skipped_records: 0,
                duplicates: 0,
            }],
            Vec::new(),
        )
    };

    for status in [AiCoverageStatus::Ok, AiCoverageStatus::Partial] {
        assert_eq!(
            with_coverage(status, 0).unwrap().replaced_providers(),
            [AiProvider::Claude]
        );
        assert_eq!(
            message(with_coverage(status, 1)),
            "coverage[0]: ok or partial coverage must have no unreadable history"
        );
    }

    for status in [AiCoverageStatus::Failed, AiCoverageStatus::Missing] {
        assert!(with_coverage(status, 1)
            .unwrap()
            .replaced_providers()
            .is_empty());
    }
}

#[test]
fn totals_have_no_estimate_while_any_usage_is_unpriced() {
    let priced = AiUsageTotals {
        records: 3,
        priced_cost_usd: 1.5,
        ..AiUsageTotals::default()
    };
    assert_eq!(priced.estimated_cost_usd(), Some(1.5));
    assert_eq!(
        AiUsageTotals {
            unpriced_buckets: 1,
            unpriced_records: 2,
            ..priced
        }
        .estimated_cost_usd(),
        None
    );
}

#[test]
fn machine_ids_and_time_zones_are_parsed_strictly() {
    assert!(AiMachineId::parse("5F0C5A1E-3B8E-4D8E-9A57-0D7B1C1F2E3A").is_some());
    assert!(AiMachineId::parse("5f0c5a1e3b8e4d8e9a570d7b1c1f2e3a").is_none());
    assert!(AiMachineId::parse("{5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a}").is_none());

    assert!(AiUsageTimeZone::parse("Europe/Stockholm").is_some());
    assert!(AiUsageTimeZone::parse("America/Argentina/Buenos_Aires").is_some());
    assert!(AiUsageTimeZone::parse("").is_none());
    assert!(AiUsageTimeZone::parse("Europe/Stockholm'; --").is_none());
}

#[test]
fn date_ranges_convert_to_and_from_inclusive_ends() {
    use time::macros::date;

    let september =
        AiUsageDateRange::from_inclusive(date!(2026 - 09 - 01), date!(2026 - 09 - 30)).unwrap();
    assert_eq!(
        (september.start(), september.end(), september.last_day()),
        (
            date!(2026 - 09 - 01),
            date!(2026 - 10 - 01),
            date!(2026 - 09 - 30)
        )
    );

    let one_day =
        AiUsageDateRange::from_inclusive(date!(2028 - 02 - 29), date!(2028 - 02 - 29)).unwrap();
    assert_eq!(one_day.end(), date!(2028 - 03 - 01));
    assert_eq!(one_day.last_day(), date!(2028 - 02 - 29));

    assert!(
        AiUsageDateRange::from_inclusive(date!(2026 - 09 - 02), date!(2026 - 09 - 01)).is_none()
    );
}

fn bucket_at(hour: u8) -> AiUsageBucket {
    let mut bucket = bucket();
    bucket.hour_start = bucket.hour_start.replace_hour(hour).unwrap();
    bucket
}
