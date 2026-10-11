//! AI token usage uploaded by developer machines.
//!
//! A machine uploads hourly, per-session buckets for a half-open window of whole
//! UTC hours. For each provider whose history it read (`ok` or `partial`
//! coverage), the upload replaces what that machine stored for the provider inside
//! the window; other providers keep their stored usage, because missing data is
//! not zero. Costs are API-equivalent USD estimates, not bills: an unknown cost
//! is `None`, never zero.

use std::{collections::HashSet, fmt};

use time::{Date, OffsetDateTime, UtcOffset};
use uuid::Uuid;

use super::AiMappedProject;
use crate::domain::AiUsageError;

const SECONDS_PER_HOUR: i64 = 3_600;
const SESSION_KEY_LEN: usize = 32;
const MAX_TIME_ZONE_LEN: usize = 64;
// Effect Schema.Natural uses JavaScript safe integers; keep the upload boundary identical.
pub(crate) const MAX_SAFE_COUNT: i64 = 9_007_199_254_740_991;
const NOT_REPLACED: &str = "provider must be listed in coverage as ok or partial";
/// The longest uploaded text, in characters; token-ledger truncates to fit.
pub const MAX_TEXT_CHARS: usize = 512;

/// A random, stable identifier created once per token-ledger installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiMachineId(Uuid);

impl AiMachineId {
    /// Parses the hyphenated form, such as `5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a`.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.len() != 36 {
            return None;
        }

        Uuid::try_parse(raw).ok().map(Self)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for AiMachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// A tool whose local history token-ledger reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiProvider {
    Codex,
    Claude,
    Grok,
    Copilot,
}

impl AiProvider {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "grok" => Some(Self::Grok),
            "copilot" => Some(Self::Copilot),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Copilot => "copilot",
        }
    }
}

/// How completely a machine could read one provider's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCoverageStatus {
    Ok,
    Partial,
    Failed,
    /// No history was found.
    Missing,
}

impl AiCoverageStatus {
    /// Whether an upload is authoritative for the provider's usage inside its
    /// window. Missing or failed history proves nothing, so stored usage stays.
    pub fn replaces_usage(self) -> bool {
        matches!(self, Self::Ok | Self::Partial)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Missing => "missing",
        }
    }
}

/// Where the rates behind the cost estimates came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPricingStatus {
    Fresh,
    Cached,
    Unavailable,
    Custom,
}

impl AiPricingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Cached => "cached",
            Self::Unavailable => "unavailable",
            Self::Custom => "custom",
        }
    }
}

/// The uploading installation. The label is an editable display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMachine {
    pub id: AiMachineId,
    pub label: String,
    pub client_version: String,
    /// The client's IANA zone, for display only.
    pub time_zone: String,
}

/// Provenance of the rates behind every cost estimate in an upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPricing {
    pub status: AiPricingStatus,
    /// When the rates were fetched, if they were.
    pub fetched_at: Option<OffsetDateTime>,
    pub source: String,
}

/// Disjoint token categories: `input` excludes cache reads and writes, and
/// reasoning is inside `output`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiTokenCounts {
    pub input: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub output: i64,
}

/// One session's usage of one model during one UTC hour.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageBucket {
    pub hour_start: OffsetDateTime,
    /// 32 lowercase hex characters; opaque and stable per session.
    pub session_key: String,
    /// A configured project name, `host/owner/repo`, or `unattributed`.
    pub project_key: String,
    pub provider: AiProvider,
    pub model: String,
    pub tokens: AiTokenCounts,
    pub records: i64,
    /// `None` when some records could not be priced.
    pub estimated_cost_usd: Option<f64>,
    pub unpriced_records: i64,
}

/// One provider's history coverage on the uploading machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderCoverage {
    pub provider: AiProvider,
    pub status: AiCoverageStatus,
    pub files: i64,
    /// Existing history paths that could not be read.
    pub unreadable: i64,
    pub malformed_lines: i64,
    pub skipped_records: i64,
    pub duplicates: i64,
}

/// A billing plan a provider reported during an hour, such as Codex `plan_type`.
/// Evidence only: declared subscriptions remain authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiProviderHint {
    pub provider: AiProvider,
    pub hour_start: OffsetDateTime,
    pub plan: String,
}

/// A half-open `[start, end)` range of whole UTC hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiUsageWindow {
    start: OffsetDateTime,
    end: OffsetDateTime,
}

impl AiUsageWindow {
    pub fn new(start: OffsetDateTime, end: OffsetDateTime) -> Result<Self, AiUsageError> {
        let start = whole_utc_hour(start)
            .ok_or_else(|| invalid("window.start must be a whole UTC hour"))?;
        let end =
            whole_utc_hour(end).ok_or_else(|| invalid("window.end must be a whole UTC hour"))?;
        if start >= end {
            return Err(invalid("window.start must be before window.end"));
        }

        Ok(Self { start, end })
    }

    pub fn start(&self) -> OffsetDateTime {
        self.start
    }

    pub fn end(&self) -> OffsetDateTime {
        self.end
    }

    pub fn contains(&self, instant: OffsetDateTime) -> bool {
        self.start <= instant && instant < self.end
    }
}

/// A validated upload from one machine.
///
/// The upload replaces stored usage only for the providers it covers as `ok` or
/// `partial`, and only inside its window. Every bucket and hint therefore belongs
/// to such a provider and lies inside the window. Bucket keys `(hour_start,
/// session_key, project_key, provider, model)` are unique, each provider has at
/// most one coverage entry, and hints are distinct.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageUpload {
    machine: AiMachine,
    window: AiUsageWindow,
    pricing: AiPricing,
    buckets: Vec<AiUsageBucket>,
    coverage: Vec<AiProviderCoverage>,
    provider_hints: Vec<AiProviderHint>,
}

impl AiUsageUpload {
    pub fn new(
        machine: AiMachine,
        window: AiUsageWindow,
        pricing: AiPricing,
        buckets: Vec<AiUsageBucket>,
        coverage: Vec<AiProviderCoverage>,
        provider_hints: Vec<AiProviderHint>,
    ) -> Result<Self, AiUsageError> {
        for (field, value, required) in [
            ("machine.label", &machine.label, true),
            ("clientVersion", &machine.client_version, true),
            ("timeZone", &machine.time_zone, true),
            ("pricing.source", &pricing.source, false),
        ] {
            if let Some(problem) = text_problem(field, value, required) {
                return Err(invalid(problem));
            }
        }

        let mut covered = HashSet::with_capacity(coverage.len());
        for (index, entry) in coverage.iter().enumerate() {
            if !covered.insert(entry.provider) {
                return Err(invalid(format!(
                    "coverage[{index}]: provider {} is listed more than once",
                    entry.provider.as_str()
                )));
            }
            if [
                entry.files,
                entry.unreadable,
                entry.malformed_lines,
                entry.skipped_records,
                entry.duplicates,
            ]
            .iter()
            .any(|count| !is_safe_count(*count))
            {
                return Err(invalid(format!(
                    "coverage[{index}]: counts must be non-negative safe integers"
                )));
            }
            if entry.status.replaces_usage() && entry.unreadable != 0 {
                return Err(invalid(format!(
                    "coverage[{index}]: ok or partial coverage must have no unreadable history"
                )));
            }
        }
        let replaced = coverage
            .iter()
            .filter(|entry| entry.status.replaces_usage())
            .map(|entry| entry.provider)
            .collect::<HashSet<_>>();

        let buckets = buckets
            .into_iter()
            .enumerate()
            .map(|(index, bucket)| {
                validate_bucket(bucket, &window, &replaced)
                    .map_err(|problem| invalid(format!("buckets[{index}]: {problem}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut keys = HashSet::with_capacity(buckets.len());
        for (index, bucket) in buckets.iter().enumerate() {
            let key = (
                bucket.hour_start,
                bucket.session_key.as_str(),
                bucket.project_key.as_str(),
                bucket.provider,
                bucket.model.as_str(),
            );
            if !keys.insert(key) {
                return Err(invalid(format!(
                    "buckets[{index}]: duplicate key (hourStart, sessionKey, project, provider, model)"
                )));
            }
        }

        let mut seen = HashSet::with_capacity(provider_hints.len());
        let mut hints = Vec::with_capacity(provider_hints.len());
        for (index, mut hint) in provider_hints.into_iter().enumerate() {
            let problem = |problem: &str| invalid(format!("providerHints[{index}]: {problem}"));
            hint.hour_start = whole_utc_hour(hint.hour_start)
                .ok_or_else(|| problem("hourStart must be a whole UTC hour"))?;
            if !window.contains(hint.hour_start) {
                return Err(problem("hourStart must be inside the window"));
            }
            if !replaced.contains(&hint.provider) {
                return Err(problem(NOT_REPLACED));
            }
            if let Some(text) = text_problem("plan", &hint.plan, true) {
                return Err(problem(&text));
            }
            if !seen.insert(hint.clone()) {
                return Err(problem("duplicate hint (provider, hourStart, plan)"));
            }
            hints.push(hint);
        }

        Ok(Self {
            machine,
            window,
            pricing,
            buckets,
            coverage,
            provider_hints: hints,
        })
    }

    /// Providers whose stored usage inside the window this upload replaces.
    pub fn replaced_providers(&self) -> Vec<AiProvider> {
        self.coverage
            .iter()
            .filter(|entry| entry.status.replaces_usage())
            .map(|entry| entry.provider)
            .collect()
    }

    pub fn machine(&self) -> &AiMachine {
        &self.machine
    }

    pub fn window(&self) -> &AiUsageWindow {
        &self.window
    }

    pub fn pricing(&self) -> &AiPricing {
        &self.pricing
    }

    pub fn buckets(&self) -> &[AiUsageBucket] {
        &self.buckets
    }

    pub fn coverage(&self) -> &[AiProviderCoverage] {
        &self.coverage
    }

    pub fn provider_hints(&self) -> &[AiProviderHint] {
        &self.provider_hints
    }
}

/// The outcome of storing an upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiUsageIngestReceipt {
    pub stored_buckets: u64,
}

/// An IANA time zone, such as `Europe/Stockholm`, for usage days and billing months.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiUsageTimeZone(String);

impl AiUsageTimeZone {
    /// Checks the shape of a zone name; the database resolves the zone itself.
    pub fn parse(raw: &str) -> Option<Self> {
        let valid = !raw.is_empty()
            && raw.len() <= MAX_TIME_ZONE_LEN
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/_+-".contains(&byte));

        valid.then(|| Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The calendar unit usage totals are grouped by, in the configured time zone.
///
/// An hourly bucket belongs to the local day of the instant it starts. In zones
/// whose offset is not a whole number of hours, a bucket can straddle local
/// midnight; it is not split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiUsagePeriod {
    Day,
    Month,
}

/// A half-open `[start, end)` range of local calendar dates: `end` is the first
/// day *after* the range. Subscription periods and API date parameters end
/// inclusively instead; convert with `from_inclusive`, `last_day` and
/// `AiSubscriptionPeriod::to_date_range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiUsageDateRange {
    start: Date,
    end: Date,
}

impl AiUsageDateRange {
    pub fn new(start: Date, end: Date) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }

    /// The range from `first_day` through `last_day`, both inclusive, or `None`
    /// when `last_day` is before `first_day`.
    pub fn from_inclusive(first_day: Date, last_day: Date) -> Option<Self> {
        Self::new(first_day, last_day.next_day()?)
    }

    /// The last day in the range, inclusive: the day before `end`.
    pub fn last_day(&self) -> Date {
        // `end` is after `start`, so it has a previous day.
        self.end.previous_day().unwrap_or(self.start)
    }

    pub fn start(&self) -> Date {
        self.start
    }

    pub fn end(&self) -> Date {
        self.end
    }
}

/// Usage summed across buckets and machines.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AiUsageTotals {
    pub tokens: AiTokenCounts,
    pub records: i64,
    /// The sum of the known costs only.
    pub priced_cost_usd: f64,
    /// Buckets whose cost is unknown.
    pub unpriced_buckets: i64,
    pub unpriced_records: i64,
}

impl AiUsageTotals {
    /// The estimated total, or `None` when any usage is unpriced. In that case
    /// `priced_cost_usd` is a known subtotal, not a total.
    pub fn estimated_cost_usd(&self) -> Option<f64> {
        (self.unpriced_buckets == 0 && self.unpriced_records == 0).then_some(self.priced_cost_usd)
    }
}

/// A user's usage of one project during one local day or month.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsagePeriodTotals {
    /// The local dates these totals cover: the day or month, clipped to the
    /// requested range. A month the range covers only partly is therefore visible
    /// as partial rather than labelled as the whole month.
    pub dates: AiUsageDateRange,
    pub project_key: String,
    /// The project the key is mapped to when the totals are read, so a mapping
    /// applies retroactively. `None` is unassigned, as `unattributed` usage always is.
    pub project: Option<AiMappedProject>,
    pub totals: AiUsageTotals,
}

fn validate_bucket(
    mut bucket: AiUsageBucket,
    window: &AiUsageWindow,
    replaced: &HashSet<AiProvider>,
) -> Result<AiUsageBucket, String> {
    bucket.hour_start =
        whole_utc_hour(bucket.hour_start).ok_or("hourStart must be a whole UTC hour")?;
    if !window.contains(bucket.hour_start) {
        return Err("hourStart must be inside the window".to_string());
    }
    if !replaced.contains(&bucket.provider) {
        return Err(NOT_REPLACED.to_string());
    }
    if !is_session_key(&bucket.session_key) {
        return Err("sessionKey must be 32 lowercase hex characters".to_string());
    }
    for (field, value, required) in [
        ("project", &bucket.project_key, true),
        ("model", &bucket.model, false),
    ] {
        if let Some(problem) = text_problem(field, value, required) {
            return Err(problem);
        }
    }

    let AiTokenCounts {
        input,
        cache_read,
        cache_write,
        output,
    } = bucket.tokens;
    if [input, cache_read, cache_write, output]
        .iter()
        .any(|count| !is_safe_count(*count))
    {
        return Err("token counts must be non-negative safe integers".to_string());
    }
    if !(1..=MAX_SAFE_COUNT).contains(&bucket.records) {
        return Err("records must be positive safe integers".to_string());
    }
    if !is_safe_count(bucket.unpriced_records) {
        return Err("unpricedRecords must be non-negative safe integers".to_string());
    }
    if bucket.unpriced_records > bucket.records {
        return Err("unpricedRecords must not exceed records".to_string());
    }
    if bucket
        .estimated_cost_usd
        .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
    {
        return Err("estimatedCostUsd must be a non-negative number or null".to_string());
    }
    if bucket.estimated_cost_usd.is_none() != (bucket.unpriced_records > 0) {
        return Err(
            "estimatedCostUsd must be null exactly when unpricedRecords is positive".to_string(),
        );
    }

    Ok(bucket)
}

fn whole_utc_hour(instant: OffsetDateTime) -> Option<OffsetDateTime> {
    let instant = instant.to_offset(UtcOffset::UTC);
    (instant.unix_timestamp() % SECONDS_PER_HOUR == 0 && instant.nanosecond() == 0)
        .then_some(instant)
}

fn is_safe_count(count: i64) -> bool {
    (0..=MAX_SAFE_COUNT).contains(&count)
}

/// Describes why uploaded text cannot be stored, if it cannot: it is empty when
/// required, longer than `MAX_TEXT_CHARS`, or contains NUL, which JSON strings can
/// carry but Postgres text cannot.
pub(super) fn text_problem(field: &str, text: &str, required: bool) -> Option<String> {
    if required && text.is_empty() {
        Some(format!("{field} must not be empty"))
    } else if text.chars().count() > MAX_TEXT_CHARS {
        Some(format!(
            "{field} must be at most {MAX_TEXT_CHARS} characters"
        ))
    } else if text.contains('\0') {
        Some(format!("{field} must not contain NUL characters"))
    } else {
        None
    }
}

fn is_session_key(key: &str) -> bool {
    key.len() == SESSION_KEY_LEN
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(message: impl Into<String>) -> AiUsageError {
    AiUsageError::InvalidUpload(message.into())
}

#[cfg(test)]
mod tests;
