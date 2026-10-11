//! Whether developers' machines have uploaded all of a month's AI usage, so
//! billing is not run on incomplete data.
//!
//! # What counts as uploaded
//!
//! Each sync that replaces a provider's usage (coverage `ok` or `partial`) is
//! logged as an interval: from the start of its window until the end of its
//! window or the moment it was reported, whichever is earlier. Usage recorded
//! after an upload cannot be in it, so a sync at 10:00 on 30 September covers
//! 30 September only until 10:00. A machine has uploaded a local day for a
//! provider when the union of that provider's intervals spans the whole day,
//! whatever its length on a daylight-saving change.
//!
//! # What a month requires
//!
//! The required days run from the month's first day through its last day or,
//! while the month is in progress, through yesterday. For each machine that
//! is *active* in the month (it stored usage in the month, or synced after the
//! month began):
//!
//! - every required day must be uploaded for at least one provider, so a
//!   machine that stopped syncing, or synced with gaps between windows, is
//!   caught even for providers it has no usage of; and
//! - every *expected* provider must be uploaded on every required day. A
//!   provider is expected when the machine stored usage of it in the month, or
//!   when one of its syncs replaced the provider on a required day. So a sync
//!   of only some providers (`token-ledger sync --provider`) leaves the others
//!   incomplete, and a provider whose history later fails to read or goes
//!   missing is flagged rather than assumed idle. A provider the machine never
//!   read, such as one reported `missing` from the start, is not expected.
//!
//! A machine that neither stored usage in the month nor synced after it began
//! is inactive: its days cannot be told apart from an unused machine's, so it
//! does not hold the month back; it is shown with its last sync date and as
//! stale. Usage uploaded before the sync log existed has no intervals except
//! the latest window of each provider, recorded when the log was added, so
//! earlier days of such machines show as not uploaded rather than complete.

use std::collections::{BTreeMap, HashSet};

use time::{Date, OffsetDateTime};

use super::{
    AiBillingMonth, AiCoverageStatus, AiMachineId, AiPricingStatus, AiProvider, AiSubscription,
    AiUsageDateRange, UserId, MACHINE_STALE_AFTER,
};

/// Providers in display order.
const PROVIDERS: [AiProvider; 4] = [
    AiProvider::Codex,
    AiProvider::Claude,
    AiProvider::Grok,
    AiProvider::Copilot,
];

/// A local calendar day and the instants it spans in the configured time zone:
/// 23 or 25 hours on daylight-saving changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiLocalDay {
    pub day: Date,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
}

/// What one sync proved: the machine's stored usage of `provider` is complete
/// from `start` until `end`, the window's end capped at the report time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiCoverageInterval {
    pub provider: AiProvider,
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
}

/// The latest coverage a machine reported for one provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiProviderReport {
    pub provider: AiProvider,
    pub status: AiCoverageStatus,
    /// `None` until an upload first replaced the provider's usage.
    pub pricing_status: Option<AiPricingStatus>,
}

/// Someone whose AI usage or subscriptions billing can concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBillingDeveloper {
    pub user_id: UserId,
    pub full_name: String,
    pub email: String,
}

/// A developer's machine as completeness sees it for one month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBillingMachine {
    pub user_id: UserId,
    pub machine_id: AiMachineId,
    pub label: String,
    pub client_version: String,
    /// When the machine last synced; for staleness only. Admins see the date.
    pub last_synced_at: OffsetDateTime,
    /// The local date of the last sync.
    pub last_synced_on: Date,
    /// The machine synced on or after the month's first local midnight.
    pub synced_since_month_start: bool,
    /// Providers the machine stored usage of in the month.
    pub providers_with_usage: Vec<AiProvider>,
    pub latest_reports: Vec<AiProviderReport>,
    /// Logged syncs whose intervals overlap the month.
    pub intervals: Vec<AiCoverageInterval>,
}

/// Required days a machine has not uploaded for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderCompleteness {
    pub provider: AiProvider,
    pub latest_report: Option<AiProviderReport>,
    /// Billing needs this provider's uploads from the machine for the month.
    pub expected: bool,
    /// Runs of required days not uploaded; always empty when not expected.
    pub gaps: Vec<AiUsageDateRange>,
}

/// Whether one machine's uploads cover a month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMachineCompleteness {
    pub machine: AiBillingMachine,
    /// No sync for longer than `MACHINE_STALE_AFTER`, as on the personal view.
    pub stale: bool,
    /// The machine stored usage in the month or synced after it began.
    pub active_in_month: bool,
    /// Runs of required days that no provider uploaded.
    pub gaps: Vec<AiUsageDateRange>,
    /// Expected providers, and any other provider the machine reports.
    pub providers: Vec<AiProviderCompleteness>,
    /// No gaps, for the machine or any expected provider.
    pub complete: bool,
}

/// Whether billing a month can rely on a developer's uploads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiDeveloperReadiness {
    /// Every machine active in the month uploaded every required day.
    Ready,
    /// Some active machine is complete, but another is not.
    Incomplete,
    /// No active machine is complete, or the developer never synced at all.
    NoSync,
    /// None of the developer's machines was active in the month.
    NoActivity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDeveloperCompleteness {
    pub developer: AiBillingDeveloper,
    pub readiness: AiDeveloperReadiness,
    pub has_usage: bool,
    /// A subscription of theirs covers days of the month.
    pub has_subscription: bool,
    pub machines: Vec<AiMachineCompleteness>,
}

/// Whose uploads a month's billing can rely on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthCompleteness {
    pub month: AiBillingMonth,
    /// The last required day: the month's last day or, while it is in
    /// progress, yesterday. `None` before any day of the month has passed.
    pub required_through: Option<Date>,
    /// Today is in the month or before it, so its usage is still growing.
    pub in_progress: bool,
    /// No sync, then incomplete, then ready, then no activity; then by name.
    pub developers: Vec<AiDeveloperCompleteness>,
}

/// Judges whether each developer's machines have uploaded all of a month's
/// usage (see the module documentation).
///
/// `days` are the month's local days with the instants they span; `today` and
/// `now` are the store's clock. Only machines of listed developers count.
pub fn assess_completeness(
    month: AiBillingMonth,
    days: &[AiLocalDay],
    today: Date,
    now: OffsetDateTime,
    developers: Vec<AiBillingDeveloper>,
    machines: Vec<AiBillingMachine>,
    subscriptions: &[AiSubscription],
) -> AiMonthCompleteness {
    let yesterday = today.previous_day().unwrap_or(today);
    let required_through =
        Some(month.last_day().min(yesterday)).filter(|through| *through >= month.first_day());
    let required: Vec<AiLocalDay> = days
        .iter()
        .filter(|day| required_through.is_some_and(|through| day.day <= through))
        .copied()
        .collect();
    let stale_before = now - MACHINE_STALE_AFTER;
    let subscribed: HashSet<UserId> = subscriptions
        .iter()
        .filter(|subscription| {
            subscription
                .terms
                .period
                .to_date_range(month.dates())
                .is_some()
        })
        .map(|subscription| subscription.user_id)
        .collect();

    let mut by_user: BTreeMap<i32, Vec<AiMachineCompleteness>> = BTreeMap::new();
    for machine in machines {
        let completeness = assess_machine(machine, &required, stale_before);
        by_user
            .entry(completeness.machine.user_id.as_i32())
            .or_default()
            .push(completeness);
    }

    let mut developers: Vec<AiDeveloperCompleteness> = developers
        .into_iter()
        .map(|developer| {
            let machines = by_user
                .remove(&developer.user_id.as_i32())
                .unwrap_or_default();
            AiDeveloperCompleteness {
                readiness: readiness(&machines),
                has_usage: machines
                    .iter()
                    .any(|machine| !machine.machine.providers_with_usage.is_empty()),
                has_subscription: subscribed.contains(&developer.user_id),
                machines,
                developer,
            }
        })
        .collect();
    let rank = |readiness: AiDeveloperReadiness| match readiness {
        AiDeveloperReadiness::NoSync => 0,
        AiDeveloperReadiness::Incomplete => 1,
        AiDeveloperReadiness::Ready => 2,
        AiDeveloperReadiness::NoActivity => 3,
    };
    developers.sort_by(|a, b| {
        rank(a.readiness)
            .cmp(&rank(b.readiness))
            .then_with(|| a.developer.full_name.cmp(&b.developer.full_name))
            .then_with(|| {
                a.developer
                    .user_id
                    .as_i32()
                    .cmp(&b.developer.user_id.as_i32())
            })
    });

    AiMonthCompleteness {
        month,
        required_through,
        in_progress: month.last_day() >= today,
        developers,
    }
}

fn readiness(machines: &[AiMachineCompleteness]) -> AiDeveloperReadiness {
    let active: Vec<&AiMachineCompleteness> = machines
        .iter()
        .filter(|machine| machine.active_in_month)
        .collect();
    if machines.is_empty() {
        AiDeveloperReadiness::NoSync
    } else if active.is_empty() {
        AiDeveloperReadiness::NoActivity
    } else if active.iter().all(|machine| machine.complete) {
        AiDeveloperReadiness::Ready
    } else if active.iter().any(|machine| machine.complete) {
        AiDeveloperReadiness::Incomplete
    } else {
        AiDeveloperReadiness::NoSync
    }
}

fn assess_machine(
    machine: AiBillingMachine,
    required: &[AiLocalDay],
    stale_before: OffsetDateTime,
) -> AiMachineCompleteness {
    let span = required.first().zip(required.last());
    let overlaps_required = |interval: &AiCoverageInterval| {
        span.is_some_and(|(first, last)| interval.start < last.end && interval.end > first.start)
    };
    let expected: Vec<AiProvider> = PROVIDERS
        .into_iter()
        .filter(|provider| {
            machine.providers_with_usage.contains(provider)
                || machine
                    .intervals
                    .iter()
                    .any(|interval| interval.provider == *provider && overlaps_required(interval))
        })
        .collect();

    let providers: Vec<AiProviderCompleteness> = PROVIDERS
        .into_iter()
        .filter_map(|provider| {
            let latest_report = machine
                .latest_reports
                .iter()
                .find(|report| report.provider == provider)
                .copied();
            let expected = expected.contains(&provider);
            (expected || latest_report.is_some()).then(|| AiProviderCompleteness {
                provider,
                latest_report,
                expected,
                gaps: if expected {
                    gaps(
                        machine
                            .intervals
                            .iter()
                            .filter(|interval| interval.provider == provider),
                        required,
                    )
                } else {
                    Vec::new()
                },
            })
        })
        .collect();
    let machine_gaps = gaps(machine.intervals.iter(), required);
    let complete =
        machine_gaps.is_empty() && providers.iter().all(|provider| provider.gaps.is_empty());

    AiMachineCompleteness {
        stale: machine.last_synced_at < stale_before,
        active_in_month: !machine.providers_with_usage.is_empty()
            || machine.synced_since_month_start,
        gaps: machine_gaps,
        providers,
        complete,
        machine,
    }
}

/// Runs of `days` that the union of `intervals` does not wholly span.
fn gaps<'a>(
    intervals: impl Iterator<Item = &'a AiCoverageInterval>,
    days: &[AiLocalDay],
) -> Vec<AiUsageDateRange> {
    let mut sorted: Vec<(OffsetDateTime, OffsetDateTime)> = intervals
        .filter(|interval| interval.start < interval.end)
        .map(|interval| (interval.start, interval.end))
        .collect();
    sorted.sort();
    let mut merged: Vec<(OffsetDateTime, OffsetDateTime)> = Vec::with_capacity(sorted.len());
    for (start, end) in sorted {
        match merged.last_mut() {
            // Overlapping or touching intervals leave no instant uncovered.
            Some((_, last_end)) if start <= *last_end => *last_end = (*last_end).max(end),
            _ => merged.push((start, end)),
        }
    }

    let mut runs: Vec<(Date, Date)> = Vec::new();
    for day in days {
        let covered = merged
            .iter()
            .any(|(start, end)| *start <= day.start && day.end <= *end);
        if covered {
            continue;
        }
        match runs.last_mut() {
            Some((_, last)) if last.next_day() == Some(day.day) => *last = day.day,
            _ => runs.push((day.day, day.day)),
        }
    }

    runs.into_iter()
        .filter_map(|(first, last)| AiUsageDateRange::from_inclusive(first, last))
        .collect()
}

#[cfg(test)]
mod tests {
    use time::{
        macros::{date, datetime},
        Duration,
    };

    use super::*;
    use crate::domain::models::{
        AiCurrency, AiMonthlyCost, AiSubscriptionId, AiSubscriptionPeriod, AiSubscriptionPlan,
        AiSubscriptionTerms,
    };

    use AiProvider::{Claude, Codex, Grok};

    const LAPTOP: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

    /// Stockholm's local days of a 2026 month: UTC+2 from 29 March to 25
    /// October, UTC+1 otherwise.
    fn stockholm_days(month: &str) -> Vec<AiLocalDay> {
        let month = AiBillingMonth::parse(month).unwrap();
        let midnight = |day: Date| {
            let summer = date!(2026 - 03 - 30) <= day && day <= date!(2026 - 10 - 25);
            day.midnight().assume_utc() - Duration::hours(if summer { 2 } else { 1 })
        };
        let mut days = Vec::new();
        let mut day = month.first_day();
        while month.contains(day) {
            let next = day.next_day().unwrap();
            days.push(AiLocalDay {
                day,
                start: midnight(day),
                end: midnight(next),
            });
            day = next;
        }
        days
    }

    /// Stockholm local time in September 2026 (UTC+2).
    fn september(day: u8, hour: u8) -> OffsetDateTime {
        Date::from_calendar_date(2026, time::Month::September, day)
            .unwrap()
            .with_hms(hour, 0, 0)
            .unwrap()
            .assume_utc()
            - Duration::hours(2)
    }

    /// A sync of `provider` from local midnight of 1 September through `end`.
    fn synced(
        provider: AiProvider,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> AiCoverageInterval {
        AiCoverageInterval {
            provider,
            start,
            end,
        }
    }

    fn developer(id: i32, full_name: &str) -> AiBillingDeveloper {
        AiBillingDeveloper {
            user_id: UserId::new(id),
            full_name: full_name.to_string(),
            email: format!("{id}@example.com"),
        }
    }

    fn machine(user: i32, index: u8, intervals: Vec<AiCoverageInterval>) -> AiBillingMachine {
        let providers_with_usage: Vec<AiProvider> = PROVIDERS
            .into_iter()
            .filter(|provider| {
                intervals
                    .iter()
                    .any(|interval| interval.provider == *provider)
            })
            .collect();
        AiBillingMachine {
            user_id: UserId::new(user),
            machine_id: AiMachineId::parse(&format!("{}{index:02x}", &LAPTOP[..34])).unwrap(),
            label: format!("machine-{index}"),
            client_version: "0.3.0".to_string(),
            last_synced_at: datetime!(2026-10-02 08:00 UTC),
            last_synced_on: date!(2026 - 10 - 02),
            synced_since_month_start: true,
            latest_reports: providers_with_usage
                .iter()
                .map(|provider| AiProviderReport {
                    provider: *provider,
                    status: AiCoverageStatus::Ok,
                    pricing_status: Some(AiPricingStatus::Fresh),
                })
                .collect(),
            providers_with_usage,
            intervals,
        }
    }

    /// Assesses September 2026 as of 5 October.
    fn assess_september(
        developers: Vec<AiBillingDeveloper>,
        machines: Vec<AiBillingMachine>,
        subscriptions: &[AiSubscription],
    ) -> AiMonthCompleteness {
        assess_completeness(
            AiBillingMonth::parse("2026-09").unwrap(),
            &stockholm_days("2026-09"),
            date!(2026 - 10 - 05),
            datetime!(2026-10-05 08:00 UTC),
            developers,
            machines,
            subscriptions,
        )
    }

    fn one_machine(machine: AiBillingMachine) -> AiMachineCompleteness {
        let completeness = assess_september(vec![developer(1, "Ada")], vec![machine], &[]);
        completeness.developers[0].machines[0].clone()
    }

    /// Gaps as inclusive `(first, last)` days.
    fn runs(gaps: &[AiUsageDateRange]) -> Vec<(Date, Date)> {
        gaps.iter()
            .map(|gap| (gap.start(), gap.last_day()))
            .collect()
    }

    /// `(provider, expected, gaps)` of each provider a machine lists.
    type ProviderGaps = (AiProvider, bool, Vec<(Date, Date)>);

    fn provider_gaps(machine: &AiMachineCompleteness) -> Vec<ProviderGaps> {
        machine
            .providers
            .iter()
            .map(|provider| (provider.provider, provider.expected, runs(&provider.gaps)))
            .collect()
    }

    const WHOLE_SEPTEMBER: (Date, Date) = (date!(2026 - 09 - 01), date!(2026 - 09 - 30));

    #[test]
    fn a_sync_covers_its_window_only_until_it_was_reported() {
        // A 14-day window ends at the next local midnight, but a sync at
        // 10:00 on 30 September cannot hold that afternoon's usage.
        let at_ten = one_machine(machine(
            1,
            1,
            vec![synced(
                Claude,
                september(1, 0) - Duration::days(13),
                september(30, 10),
            )],
        ));
        assert!(!at_ten.complete);
        assert_eq!(
            runs(&at_ten.gaps),
            [(date!(2026 - 09 - 30), date!(2026 - 09 - 30))]
        );

        let next_morning = one_machine(machine(
            1,
            1,
            vec![
                synced(Claude, september(1, 0), september(30, 10)),
                synced(
                    Claude,
                    september(17, 0),
                    september(30, 0) + Duration::days(1),
                ),
            ],
        ));
        assert!(next_morning.complete);
        assert!(next_morning.gaps.is_empty());
    }

    #[test]
    fn gaps_between_syncs_leave_the_month_incomplete() {
        // Synced on 5 September, and next on 20 October with a 14-day window.
        let machine = one_machine(machine(
            1,
            1,
            vec![
                synced(
                    Claude,
                    september(1, 0) - Duration::days(9),
                    september(5, 10),
                ),
                synced(
                    Claude,
                    september(30, 0) + Duration::days(6),
                    september(30, 10) + Duration::days(20),
                ),
            ],
        ));

        assert!(!machine.complete);
        assert_eq!(
            runs(&machine.gaps),
            [(date!(2026 - 09 - 05), date!(2026 - 09 - 30))]
        );
        assert_eq!(
            provider_gaps(&machine),
            [(
                Claude,
                true,
                vec![(date!(2026 - 09 - 05), date!(2026 - 09 - 30))]
            )]
        );
    }

    #[test]
    fn overlapping_and_touching_syncs_join_up() {
        let machine = one_machine(machine(
            1,
            1,
            vec![
                synced(Claude, september(1, 0), september(12, 15)),
                // Starts where the last one was reported, in the middle of a day.
                synced(Claude, september(12, 15), september(20, 9)),
                synced(
                    Claude,
                    september(7, 0),
                    september(30, 0) + Duration::days(2),
                ),
            ],
        ));

        assert!(machine.complete);
        assert_eq!(provider_gaps(&machine), [(Claude, true, vec![])]);
    }

    #[test]
    fn a_provider_synced_on_its_own_leaves_the_others_behind() {
        // Codex was read with Claude until 15 September; after that only
        // `sync --provider claude` ran. Grok's history was never found.
        let mut machine = machine(
            1,
            1,
            vec![
                synced(
                    Claude,
                    september(1, 0),
                    september(30, 0) + Duration::days(1),
                ),
                synced(Codex, september(1, 0), september(16, 0)),
            ],
        );
        machine.providers_with_usage = vec![Claude];
        machine.latest_reports.push(AiProviderReport {
            provider: Grok,
            status: AiCoverageStatus::Missing,
            pricing_status: None,
        });
        let machine = one_machine(machine);

        assert!(!machine.complete);
        assert!(machine.gaps.is_empty(), "Claude uploaded every day");
        assert_eq!(
            provider_gaps(&machine),
            [
                (
                    Codex,
                    true,
                    vec![(date!(2026 - 09 - 16), date!(2026 - 09 - 30))]
                ),
                (Claude, true, vec![]),
                (Grok, false, vec![]),
            ]
        );
    }

    #[test]
    fn a_provider_with_usage_but_no_logged_sync_is_not_complete() {
        // Usage uploaded before the sync log existed, or while the provider's
        // coverage was failing, proves nothing about the other days.
        let mut legacy = machine(1, 1, Vec::new());
        legacy.providers_with_usage = vec![Codex];
        let legacy = one_machine(legacy);

        assert!(!legacy.complete);
        assert_eq!(runs(&legacy.gaps), [WHOLE_SEPTEMBER]);
        assert_eq!(
            provider_gaps(&legacy),
            [(Codex, true, vec![WHOLE_SEPTEMBER])]
        );

        // An active machine with nothing logged at all is not complete either.
        let silent = one_machine(machine(1, 2, Vec::new()));
        assert!(!silent.complete && silent.active_in_month);
        assert_eq!(runs(&silent.gaps), [WHOLE_SEPTEMBER]);
    }

    #[test]
    fn the_long_day_of_a_daylight_saving_change_needs_all_25_hours() {
        // 25 October 2026 runs from 22:00 UTC on the 24th to 23:00 UTC on the 25th.
        let october = stockholm_days("2026-10");
        let assess = |end: OffsetDateTime| {
            assess_completeness(
                AiBillingMonth::parse("2026-10").unwrap(),
                &october,
                date!(2026 - 11 - 05),
                datetime!(2026-11-05 08:00 UTC),
                vec![developer(1, "Ada")],
                vec![machine(
                    1,
                    1,
                    vec![synced(Claude, datetime!(2026-09-30 22:00 UTC), end)],
                )],
                &[],
            )
            .developers[0]
                .machines[0]
                .gaps
                .clone()
        };

        // 23:00 local on the 25th, standard time: an hour short.
        assert_eq!(
            runs(&assess(datetime!(2026-10-25 22:00 UTC))),
            [(date!(2026 - 10 - 25), date!(2026 - 10 - 31))]
        );
        assert_eq!(
            runs(&assess(datetime!(2026-10-25 23:00 UTC))),
            [(date!(2026 - 10 - 26), date!(2026 - 10 - 31))]
        );
    }

    #[test]
    fn a_month_in_progress_needs_uploads_through_yesterday() {
        let assess = |end: OffsetDateTime| {
            assess_completeness(
                AiBillingMonth::parse("2026-09").unwrap(),
                &stockholm_days("2026-09"),
                date!(2026 - 09 - 18),
                september(18, 12),
                vec![developer(1, "Ada")],
                vec![machine(1, 1, vec![synced(Claude, september(1, 0), end)])],
                &[],
            )
        };

        // Synced at 10:00 today: yesterday is whole, today is not required.
        let current = assess(september(18, 10));
        assert!(current.in_progress);
        assert_eq!(current.required_through, Some(date!(2026 - 09 - 17)));
        assert_eq!(current.developers[0].readiness, AiDeveloperReadiness::Ready);
        assert_eq!(
            assess(september(17, 10)).developers[0].readiness,
            AiDeveloperReadiness::NoSync
        );

        // Nothing of a month that has not begun is required yet.
        let future = assess_completeness(
            AiBillingMonth::parse("2026-10").unwrap(),
            &stockholm_days("2026-10"),
            date!(2026 - 09 - 18),
            september(18, 12),
            vec![developer(1, "Ada")],
            vec![machine(1, 1, Vec::new())],
            &[],
        );
        assert_eq!(future.required_through, None);
        assert!(future.in_progress);
        assert_eq!(future.developers[0].readiness, AiDeveloperReadiness::Ready);
    }

    fn subscription(user: i32) -> AiSubscription {
        AiSubscription {
            id: AiSubscriptionId::new(user),
            user_id: UserId::new(user),
            terms: AiSubscriptionTerms {
                provider: Claude,
                plan: AiSubscriptionPlan::parse("Claude Max 5x").unwrap(),
                monthly_cost: AiMonthlyCost::parse("1100").unwrap(),
                currency: AiCurrency::parse("SEK").unwrap(),
                period: AiSubscriptionPeriod::new(date!(2026 - 01 - 01), None).unwrap(),
            },
        }
    }

    #[test]
    fn developers_are_ready_only_when_every_active_machine_is_complete() {
        let whole = || {
            synced(
                Claude,
                september(1, 0),
                september(30, 0) + Duration::days(3),
            )
        };
        let half = || synced(Claude, september(1, 0), september(16, 0));
        let inactive = |user, index| {
            let mut machine = machine(user, index, Vec::new());
            machine.synced_since_month_start = false;
            machine.last_synced_at = datetime!(2026-07-30 08:00 UTC);
            machine.last_synced_on = date!(2026 - 07 - 30);
            machine
        };

        let completeness = assess_september(
            vec![
                developer(1, "Ada"),
                developer(2, "Bea"),
                developer(3, "Cid"),
                developer(4, "Dag"),
                developer(5, "Eve"),
                developer(6, "Fia"),
            ],
            vec![
                // Ready: the old laptop was not used in September.
                machine(1, 1, vec![whole()]),
                inactive(1, 2),
                machine(2, 3, vec![whole()]),
                machine(2, 4, vec![half()]),
                machine(3, 5, vec![half()]),
                // No activity: only a machine that last synced in July.
                inactive(4, 6),
                inactive(5, 7),
                // Fia never synced, but has a subscription.
            ],
            &[subscription(5), subscription(6)],
        );

        let readiness: Vec<(&str, AiDeveloperReadiness, bool, bool)> = completeness
            .developers
            .iter()
            .map(|developer| {
                (
                    developer.developer.full_name.as_str(),
                    developer.readiness,
                    developer.has_usage,
                    developer.has_subscription,
                )
            })
            .collect();
        assert_eq!(
            readiness,
            [
                ("Cid", AiDeveloperReadiness::NoSync, true, false),
                ("Fia", AiDeveloperReadiness::NoSync, false, true),
                ("Bea", AiDeveloperReadiness::Incomplete, true, false),
                ("Ada", AiDeveloperReadiness::Ready, true, false),
                ("Dag", AiDeveloperReadiness::NoActivity, false, false),
                ("Eve", AiDeveloperReadiness::NoActivity, false, true),
            ]
        );

        let ada = &completeness.developers[3].machines;
        assert_eq!(
            ada.iter()
                .map(|machine| (machine.active_in_month, machine.stale, machine.complete))
                .collect::<Vec<_>>(),
            [(true, false, true), (false, true, false)]
        );
    }
}
