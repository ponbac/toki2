//! Importing a metered payer's usage: what an import should hold, which
//! months a run imports and how, what it covered, and whether the import is
//! fresh.
//!
//! - **Slices.** An import holds one slice per account and UTC day
//!   (`AccountCredits`), per organization day (`OrgCredits`, `OrgUsage`) and
//!   per month (`Summary`), from `backfill_from` through today. Every stored
//!   account is expected from `backfill_from`, not from its first seat day:
//!   seat history starts when Toki first listed the account.
//! - **Months.** Hourly runs import the current month in `Changes` mode, plus
//!   the previous month while it is within the settle days; nightly runs, from
//!   02:00 UTC until one finishes its plan, refresh the current month in `Full`,
//!   plus the previous month for 30 days after it ends. Months holding a slice
//!   that was never fetched join in `Changes` mode, which always fetches
//!   missing days, as do months holding an interrupted check (a run stopped
//!   before finishing them).
//! - **Coverage.** A slice is complete once fetched after its period ended.
//!   A source is imported through the latest day whose expected slices, and
//!   every earlier one's, are complete. The settle lag is the longest a slice
//!   kept changing after it was complete.

use std::{collections::HashSet, fmt};

use time::{Date, Duration, OffsetDateTime, Time};

use super::{
    utc_day, AiBillingMonth, AiMeteredSource, AiProviderAccountId, AiProviderLogin, UserId,
};
use crate::domain::AiMeteredUsageError;

/// Days a slice may still change after it ends, before any lag was observed.
pub const DEFAULT_SETTLE_DAYS: u32 = 3;
/// The UTC hour from which the nightly full refresh is due.
pub const NIGHTLY_REFRESH_HOUR_UTC: u8 = 2;
/// How long after any attempt the next scheduled run is due.
pub const IMPORT_RETRY_AFTER: Duration = Duration::minutes(55);
/// How long after the previous month ends nightly runs still refresh it.
pub const PREVIOUS_MONTH_REFRESH_DAYS: i64 = 30;
/// How old the last success or the summary may be before the import is stale.
pub const IMPORT_STALE_AFTER: Duration = Duration::hours(3);
/// The days, back from today, whose late changes count toward the settle lag.
pub const SETTLE_LAG_WINDOW_DAYS: i64 = 62;
/// The most requests one run sends.
pub const MAX_REQUESTS_PER_RUN: u32 = 4_000;
/// How far a source's coverage may trail today before the import is stale.
pub const IMPORTED_THROUGH_STALE_DAYS: i64 = 2;

/// Why a run runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiImportKind {
    Hourly,
    Nightly,
    /// An admin's Import now, by the admin while their user exists.
    Admin(Option<UserId>),
}

impl AiImportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Nightly => "nightly",
            Self::Admin(_) => "admin",
        }
    }

    pub fn requested_by(self) -> Option<UserId> {
        match self {
            Self::Admin(by) => by,
            Self::Hourly | Self::Nightly => None,
        }
    }

    pub fn parse(raw: &str, requested_by: Option<UserId>) -> Option<Self> {
        match raw {
            "hourly" => Some(Self::Hourly),
            "nightly" => Some(Self::Nightly),
            "admin" => Some(Self::Admin(requested_by)),
            _ => None,
        }
    }
}

/// How a run imports a month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMonthMode {
    /// Fetches days never fetched; then, when the payer's month totals differ
    /// from the stored ones, the open days; then, if they still differ, every
    /// day.
    Changes,
    /// Fetches every day.
    Full,
}

impl AiMonthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changes => "changes",
            Self::Full => "full",
        }
    }
}

/// The months a run imports, oldest first, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiImportPlan {
    pub kind: AiImportKind,
    pub months: Vec<(AiBillingMonth, AiMonthMode)>,
}

/// When runs were attempted and succeeded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiImportHistory {
    pub last_attempt_at: Option<OffsetDateTime>,
    /// The last run that succeeded or was partial.
    pub last_success_at: Option<OffsetDateTime>,
    /// The start of the last nightly run that finished its plan: succeeded,
    /// or partial from account errors only. A run that stopped (its `error`)
    /// did not finish the full refresh.
    pub last_nightly_success_at: Option<OffsetDateTime>,
}

/// A stored slice and its fetches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSliceRecord {
    pub source: AiMeteredSource,
    /// The account of an `AccountCredits` slice.
    pub account_id: Option<AiProviderAccountId>,
    /// The UTC day, or a summary's month's first day.
    pub day: Date,
    pub fetched_at: OffsetDateTime,
    pub changed_at: OffsetDateTime,
    pub complete_at: Option<OffsetDateTime>,
    pub late_changed_at: Option<OffsetDateTime>,
}

impl AiSliceRecord {
    pub fn is_complete(&self) -> bool {
        self.complete_at.is_some()
    }
}

/// The instant a slice's period ends: the next day's midnight UTC, or the
/// next month's for a summary.
pub fn slice_end(source: AiMeteredSource, day: Date) -> Option<OffsetDateTime> {
    let end = match source {
        AiMeteredSource::Summary => AiBillingMonth::containing(day)?.dates().end(),
        AiMeteredSource::AccountCredits
        | AiMeteredSource::OrgCredits
        | AiMeteredSource::OrgUsage => day.next_day()?,
    };
    Some(end.midnight().assume_utc())
}

type SliceKey = (AiMeteredSource, Option<AiProviderAccountId>, Date);

/// A set of slices by source, account and day.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiSliceSet(HashSet<SliceKey>);

impl AiSliceSet {
    /// Every slice of `records`.
    pub fn fetched<'a>(records: impl IntoIterator<Item = &'a AiSliceRecord>) -> Self {
        Self(
            records
                .into_iter()
                .map(|record| (record.source, record.account_id, record.day))
                .collect(),
        )
    }

    /// The complete slices of `records`.
    pub fn complete<'a>(records: impl IntoIterator<Item = &'a AiSliceRecord>) -> Self {
        Self::fetched(records.into_iter().filter(|record| record.is_complete()))
    }

    pub fn contains(
        &self,
        source: AiMeteredSource,
        account_id: Option<AiProviderAccountId>,
        day: Date,
    ) -> bool {
        self.0.contains(&(source, account_id, day))
    }
}

/// Every slice an import should hold, from `backfill_from` through today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExpectedSlices {
    accounts: Vec<AiProviderAccountId>,
    backfill_from: Date,
    today: Date,
    /// (account, month's first day) whose slices are not expected.
    excused: HashSet<(AiProviderAccountId, Date)>,
}

impl AiExpectedSlices {
    /// Each of `accounts` × each UTC day, each organization day, and each
    /// month's summary, from `backfill_from` through `today`.
    pub fn new(accounts: &[AiProviderAccountId], backfill_from: Date, today: Date) -> Self {
        Self {
            accounts: accounts.to_vec(),
            backfill_from,
            today,
            excused: HashSet::new(),
        }
    }

    /// Without the given accounts' slices in the given months: accounts whose
    /// import of a month failed although they held no seat in it (departed
    /// people, say), whose errors never make the import stale or a gap.
    pub fn excusing(
        mut self,
        excused: impl IntoIterator<Item = (AiProviderAccountId, AiBillingMonth)>,
    ) -> Self {
        self.excused.extend(
            excused
                .into_iter()
                .map(|(account, month)| (account, month.first_day())),
        );
        self
    }

    /// The expected slices of `source` first due on `day`: each account's,
    /// the organization's, or the summary of `day`'s month on the month's
    /// first expected day (its slice keyed by the month's first day).
    pub fn on(
        &self,
        source: AiMeteredSource,
        day: Date,
    ) -> Vec<(Option<AiProviderAccountId>, Date)> {
        match source {
            AiMeteredSource::AccountCredits => {
                let month = AiBillingMonth::containing(day).map(|month| month.first_day());
                self.accounts
                    .iter()
                    .filter(|account| {
                        month.is_none_or(|month| !self.excused.contains(&(**account, month)))
                    })
                    .map(|account| (Some(*account), day))
                    .collect()
            }
            AiMeteredSource::OrgCredits | AiMeteredSource::OrgUsage => vec![(None, day)],
            AiMeteredSource::Summary => match AiBillingMonth::containing(day) {
                Some(month) if day == month.first_day().max(self.backfill_from) => {
                    vec![(None, month.first_day())]
                }
                _ => Vec::new(),
            },
        }
    }

    /// The expected days, `backfill_from` through `through`.
    fn days_through(&self, through: Date) -> impl Iterator<Item = Date> {
        let mut next = Some(self.backfill_from);
        std::iter::from_fn(move || {
            let day = next.filter(|day| *day <= through)?;
            next = day.next_day();
            Some(day)
        })
    }
}

/// Months holding an expected slice, before today, that was never fetched,
/// oldest first. Today's slices are the hourly run's own work, never a gap.
pub fn gap_months(expected: &AiExpectedSlices, fetched: &AiSliceSet) -> Vec<AiBillingMonth> {
    let mut months: Vec<AiBillingMonth> = Vec::new();
    let Some(yesterday) = expected.today.previous_day() else {
        return months;
    };
    for day in expected.days_through(yesterday) {
        let Some(month) = AiBillingMonth::containing(day) else {
            continue;
        };
        if months.contains(&month) {
            continue;
        }
        let missing = AiMeteredSource::ALL.into_iter().any(|source| {
            expected
                .on(source, day)
                .into_iter()
                .any(|(account, slice_day)| !fetched.contains(source, account, slice_day))
        });
        if missing {
            months.push(month);
        }
    }
    months
}

/// The run due at `now` from the schedule, or `None`.
///
/// - `None` while the last attempt is less than 55 minutes old.
/// - Nightly, when now is at or after 02:00 UTC and no nightly run finished
///   its plan since: the current month `Full`, plus the previous month `Full`
///   while today is at most 30 days after its last day.
/// - Otherwise hourly: the current month in `Changes`, plus the previous month
///   while today is at most `settle_days` after its last day.
/// - `gaps` join in `Changes`. Months before `backfill_from`'s are never
///   planned.
pub fn scheduled_plan(
    now: OffsetDateTime,
    backfill_from: Date,
    history: &AiImportHistory,
    gaps: &[AiBillingMonth],
    settle_days: u32,
) -> Option<AiImportPlan> {
    if history
        .last_attempt_at
        .is_some_and(|attempt| now - attempt < IMPORT_RETRY_AFTER)
    {
        return None;
    }
    let today = utc_day(now);
    if today < backfill_from {
        return None;
    }
    let current = AiBillingMonth::containing(today)?;
    let nightly_at = today
        .with_time(Time::from_hms(NIGHTLY_REFRESH_HOUR_UTC, 0, 0).ok()?)
        .assume_utc();
    let nightly = now >= nightly_at
        && !history
            .last_nightly_success_at
            .is_some_and(|success| success >= nightly_at);
    let (kind, mode, previous_days) = if nightly {
        (
            AiImportKind::Nightly,
            AiMonthMode::Full,
            PREVIOUS_MONTH_REFRESH_DAYS,
        )
    } else {
        (
            AiImportKind::Hourly,
            AiMonthMode::Changes,
            i64::from(settle_days),
        )
    };

    let mut months = vec![(current, mode)];
    if let Some(previous) = current.previous() {
        if (today - previous.last_day()).whole_days() <= previous_days {
            months.push((previous, mode));
        }
    }
    Some(AiImportPlan {
        kind,
        months: with_gaps(months, gaps, backfill_from),
    })
}

/// An admin's run: `month` in `mode`, plus `gaps` in `Changes`. `Invalid` for
/// a month before `backfill_from`'s or after today's.
pub fn admin_plan(
    month: AiBillingMonth,
    mode: AiMonthMode,
    today: Date,
    backfill_from: Date,
    gaps: &[AiBillingMonth],
    by: UserId,
) -> Result<AiImportPlan, AiMeteredUsageError> {
    ensure_importable(month, today, backfill_from)?;
    Ok(AiImportPlan {
        kind: AiImportKind::Admin(Some(by)),
        months: with_gaps(vec![(month, mode)], gaps, backfill_from),
    })
}

/// `Invalid` unless `month` lies from `backfill_from`'s month through
/// today's.
pub fn ensure_importable(
    month: AiBillingMonth,
    today: Date,
    backfill_from: Date,
) -> Result<(), AiMeteredUsageError> {
    let first = AiBillingMonth::containing(backfill_from);
    let last = AiBillingMonth::containing(today);
    match (first, last) {
        (Some(first), Some(last))
            if first.first_day() <= month.first_day() && month.first_day() <= last.first_day() =>
        {
            Ok(())
        }
        (Some(first), Some(last)) => Err(AiMeteredUsageError::Invalid(format!(
            "{month} is outside the imported months, {first} through {last}"
        ))),
        _ => Err(AiMeteredUsageError::Invalid(format!(
            "{month} cannot be imported"
        ))),
    }
}

fn with_gaps(
    mut months: Vec<(AiBillingMonth, AiMonthMode)>,
    gaps: &[AiBillingMonth],
    backfill_from: Date,
) -> Vec<(AiBillingMonth, AiMonthMode)> {
    for gap in gaps {
        if !months.iter().any(|(month, _)| month == gap) {
            months.push((*gap, AiMonthMode::Changes));
        }
    }
    let first = AiBillingMonth::containing(backfill_from).map(|month| month.first_day());
    months.retain(|(month, _)| first.is_some_and(|first| month.first_day() >= first));
    months.sort_by_key(|(month, _)| month.first_day());
    months
}

/// The days of `month` that may still change: from the later of its first day
/// and `today − settle_days` through the earlier of its last day and today.
pub fn open_days(month: AiBillingMonth, today: Date, settle_days: u32) -> Vec<Date> {
    let from = (today - Duration::days(i64::from(settle_days))).max(month.first_day());
    let through = today.min(month.last_day());
    let mut days = Vec::new();
    let mut day = from;
    while day <= through {
        days.push(day);
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    days
}

/// The latest UTC day `D` before today such that every expected slice of
/// `source` due on days through `D` is complete, or `None`.
pub fn imported_through(
    source: AiMeteredSource,
    expected: &AiExpectedSlices,
    complete: &AiSliceSet,
    today: Date,
) -> Option<Date> {
    let yesterday = today.previous_day()?;
    let mut through = None;
    for day in expected.days_through(yesterday) {
        let all_complete = expected
            .on(source, day)
            .into_iter()
            .all(|(account, slice_day)| complete.contains(source, account, slice_day));
        if !all_complete {
            break;
        }
        through = Some(day);
    }
    through
}

/// The longest late change, in whole days rounded up after the slice's
/// period ended, over slices of the last 62 days; `None` while there is none.
pub fn settle_lag_days(slices: &[AiSliceRecord], today: Date) -> Option<u32> {
    let window_start = today - Duration::days(SETTLE_LAG_WINDOW_DAYS);
    slices
        .iter()
        .filter(|slice| slice.day >= window_start)
        .filter_map(|slice| {
            let late = slice.late_changed_at?;
            let end = slice_end(slice.source, slice.day)?;
            let seconds = (late - end).whole_seconds().max(0);
            u32::try_from((seconds + 86_399) / 86_400).ok()
        })
        .max()
}

/// The settle days of the next run: the default, or the observed lag if
/// longer.
pub fn settle_days(observed_lag: Option<u32>) -> u32 {
    observed_lag.unwrap_or(0).max(DEFAULT_SETTLE_DAYS)
}

/// A stored run's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiImportRunId(i64);

impl AiImportRunId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }
}

/// A run's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiRunStatus {
    Running,
    Succeeded,
    /// Finished with some sources or accounts failed or skipped.
    Partial,
    Failed,
}

impl AiRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One source's outcome in a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSourceStatus {
    Succeeded,
    Partial,
    Failed,
    /// Not attempted, because the run stopped first.
    Skipped,
}

impl AiSourceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "succeeded" => Some(Self::Succeeded),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// What a run did for one source, and the source's coverage after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSourceOutcome {
    pub source: AiMeteredSource,
    pub status: AiSourceStatus,
    pub requests: u32,
    pub slices_fetched: u32,
    pub slices_changed: u32,
    pub imported_through: Option<Date>,
    pub settle_lag_days: Option<u32>,
    pub error: Option<String>,
}

/// One import run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiImportRun {
    pub id: AiImportRunId,
    pub kind: AiImportKind,
    pub months: Vec<(AiBillingMonth, AiMonthMode)>,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub status: AiRunStatus,
    pub requests: u32,
    pub slices_fetched: u32,
    pub slices_changed: u32,
    pub account_errors: u32,
    /// Why the run stopped before finishing its plan, if it did (a rate
    /// limit, the request cap, an organization-scope error, ...). Account
    /// errors that let the other accounts continue are not the run's error.
    pub error: Option<String>,
    pub sources: Vec<AiSourceOutcome>,
}

/// An account whose import failed in a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredAccountError {
    pub account_id: AiProviderAccountId,
    pub month: AiBillingMonth,
    pub error: String,
    /// Whether the run stopped before finishing the account's month, rather
    /// than the account failing on its own (see `AiUsageCheck::interrupted`).
    pub interrupted: bool,
}

/// An account whose import failed, as freshness weighs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountImportError {
    pub account_id: AiProviderAccountId,
    pub login: AiProviderLogin,
    pub month: AiBillingMonth,
    pub error: String,
    /// Whether the account held a seat in that month. Its own errors without
    /// one (departed people, say) never make the import stale.
    pub seated: bool,
    /// Whether the run stopped before finishing the account's month: no
    /// answer of the account's own, so it makes the import stale whatever
    /// the seat.
    pub interrupted: bool,
}

/// A source's coverage and last outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSourceState {
    pub source: AiMeteredSource,
    pub imported_through: Option<Date>,
    pub last_success_at: Option<OffsetDateTime>,
    /// The latest finished run's error for the source, if any.
    pub last_error: Option<String>,
    pub settle_lag_days: Option<u32>,
}

/// What the store knows about past and running imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiImportStatus {
    pub history: AiImportHistory,
    /// The run still `running` while the import lock is held. A run whose
    /// process died (a deploy, a crash) freed the lock, so it is not running,
    /// though its row says so until the next run marks it interrupted.
    pub running: Option<AiImportRun>,
    /// The latest finished run, with its sources.
    pub last_run: Option<AiImportRun>,
    /// Account errors recorded since the latest finished run started, and
    /// accounts whose month a run started since then (other than the one
    /// running) did not finish.
    pub account_errors: Vec<AiStoredAccountError>,
    /// One per source.
    pub sources: Vec<AiSourceState>,
}

impl AiImportStatus {
    /// The settle days of the next run, from the latest observed lag of
    /// account credits.
    pub fn settle_days(&self) -> u32 {
        settle_days(
            self.sources
                .iter()
                .find(|state| state.source == AiMeteredSource::AccountCredits)
                .and_then(|state| state.settle_lag_days),
        )
    }
}

/// Whether the payer's usage source can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiMeteredSourceHealth {
    Ready,
    /// No credentials are configured, so the source is off.
    Disabled,
    /// Credentials are configured but unusable, for the given reason.
    Misconfigured(String),
}

/// The import as the admin page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiImportOverview {
    pub health: AiMeteredSourceHealth,
    pub backfill_from: Date,
    /// Whether scheduled runs happen in this build.
    pub unattended: bool,
    pub freshness: AiImportFreshness,
    pub running: Option<AiImportRun>,
    pub last_run: Option<AiImportRun>,
    pub account_errors: Vec<AiAccountImportError>,
    pub sources: Vec<AiSourceState>,
}

/// A month's reconciliation with the import's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredMonth {
    pub payer: super::AiProvider,
    /// Where the payer's accounts live, such as a GitHub organization.
    pub namespace: String,
    pub today: Date,
    /// Months with imported data, oldest first.
    pub months: Vec<AiBillingMonth>,
    pub import: AiImportOverview,
    pub reconciliation: super::AiMeteredReconciliation,
}

/// Whether the import is fresh enough to trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiImportFreshness {
    Fresh,
    Stale {
        reason: AiStaleReason,
    },
    /// No run has finished.
    Never,
    /// The usage source is off or misconfigured.
    Off,
}

/// Why the import is stale. Displayed as a lower-case clause for logs; the
/// admin page phrases it itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiStaleReason {
    /// The latest finished run failed, with its error.
    LastRunFailed {
        error: String,
    },
    NeverSucceeded,
    /// The last success, more than `IMPORT_STALE_AFTER` ago.
    LastSuccessOld {
        at: OffsetDateTime,
    },
    /// A daily source with no complete day yet.
    SourceNotImported {
        source: AiMeteredSource,
    },
    /// A daily source imported only through `through`, a UTC day.
    SourceBehind {
        source: AiMeteredSource,
        through: Date,
    },
    SummaryNeverFetched,
    /// The current month's summary, last fetched more than
    /// `IMPORT_STALE_AFTER` ago.
    SummaryOld {
        at: OffsetDateTime,
    },
    /// An account that held a seat in `month` could not be imported, or a
    /// run stopped before finishing its month.
    AccountFailed {
        login: AiProviderLogin,
        month: AiBillingMonth,
        error: String,
    },
}

impl fmt::Display for AiStaleReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let instant = |at: &OffsetDateTime| {
            at.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| at.to_string())
        };
        match self {
            Self::LastRunFailed { error } => write!(f, "the last import failed: {error}"),
            Self::NeverSucceeded => write!(f, "no import has succeeded yet"),
            Self::LastSuccessOld { at } => {
                write!(f, "the last successful import was at {}", instant(at))
            }
            Self::SourceNotImported { source } => {
                write!(f, "{} are not imported yet", source.label())
            }
            Self::SourceBehind { source, through } => {
                write!(f, "{} imported through {through}", source.label())
            }
            Self::SummaryNeverFetched => write!(f, "the month summary was never fetched"),
            Self::SummaryOld { at } => {
                write!(f, "the month summary was fetched at {}", instant(at))
            }
            Self::AccountFailed {
                login,
                month,
                error,
            } => write!(f, "{login} could not be imported for {month} ({error})"),
        }
    }
}

/// What freshness is judged from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiFreshnessInput {
    pub source_ready: bool,
    pub now: OffsetDateTime,
    pub history: AiImportHistory,
    /// The latest finished run.
    pub latest_run: Option<AiImportRun>,
    pub imported_through: Vec<(AiMeteredSource, Option<Date>)>,
    /// When the current month's summary was last fetched.
    pub summary_fetched_at: Option<OffsetDateTime>,
    pub account_errors: Vec<AiAccountImportError>,
}

/// Freshness, with the first reason the import is stale:
///
/// - the latest finished run failed;
/// - the last success is more than 3 hours old;
/// - account credits, organization credits or organization usage are imported
///   through a day before `today − 2`;
/// - the current month's summary is more than 3 hours old;
/// - an account that held a seat in the month could not be imported, or a
///   run stopped before finishing an account's month.
pub fn import_freshness(input: &AiFreshnessInput) -> AiImportFreshness {
    if !input.source_ready {
        return AiImportFreshness::Off;
    }
    let Some(run) = &input.latest_run else {
        return AiImportFreshness::Never;
    };
    let stale = |reason: AiStaleReason| AiImportFreshness::Stale { reason };
    if run.status == AiRunStatus::Failed {
        return stale(AiStaleReason::LastRunFailed {
            error: run
                .error
                .clone()
                .unwrap_or_else(|| "unknown error".to_string()),
        });
    }
    match input.history.last_success_at {
        None => return stale(AiStaleReason::NeverSucceeded),
        Some(at) if input.now - at > IMPORT_STALE_AFTER => {
            return stale(AiStaleReason::LastSuccessOld { at })
        }
        Some(_) => {}
    }
    let today = utc_day(input.now);
    let oldest_fresh = today - Duration::days(IMPORTED_THROUGH_STALE_DAYS);
    for source in [
        AiMeteredSource::AccountCredits,
        AiMeteredSource::OrgCredits,
        AiMeteredSource::OrgUsage,
    ] {
        let through = input
            .imported_through
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .and_then(|(_, through)| *through);
        match through {
            None => return stale(AiStaleReason::SourceNotImported { source }),
            Some(through) if through < oldest_fresh => {
                return stale(AiStaleReason::SourceBehind { source, through })
            }
            Some(_) => {}
        }
    }
    match input.summary_fetched_at {
        None => return stale(AiStaleReason::SummaryNeverFetched),
        Some(at) if input.now - at > IMPORT_STALE_AFTER => {
            return stale(AiStaleReason::SummaryOld { at })
        }
        Some(_) => {}
    }
    if let Some(error) = input
        .account_errors
        .iter()
        .find(|error| error.seated || error.interrupted)
    {
        return stale(AiStaleReason::AccountFailed {
            login: error.login.clone(),
            month: error.month,
            error: error.error.clone(),
        });
    }
    AiImportFreshness::Fresh
}

#[cfg(test)]
mod tests;
