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

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
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

/// A tool whose local history token-ledger reads. Ordered as declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiProvider {
    Codex,
    Claude,
    Grok,
    Copilot,
    Pi,
}

impl AiProvider {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "grok" => Some(Self::Grok),
            "copilot" => Some(Self::Copilot),
            "pi" => Some(Self::Pi),
            _ => None,
        }
    }

    pub fn supports_subscription(self) -> bool {
        match self {
            Self::Pi => false,
            Self::Codex | Self::Claude | Self::Grok | Self::Copilot => true,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Grok => "grok",
            Self::Copilot => "copilot",
            Self::Pi => "pi",
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
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ok" => Some(Self::Ok),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "missing" => Some(Self::Missing),
            _ => None,
        }
    }

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
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "fresh" => Some(Self::Fresh),
            "cached" => Some(Self::Cached),
            "unavailable" => Some(Self::Unavailable),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

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
    /// Explicit subscription payer; the usage provider remains its source tool.
    pub billing_provider: Option<AiProvider>,
    pub hour_start: OffsetDateTime,
    /// 32 lowercase hex characters; opaque and stable per session.
    pub session_key: String,
    /// A configured project name, `host/owner/repo`, or `unattributed`.
    pub project_key: String,
    pub provider: AiProvider,
    pub model: String,
    pub tokens: AiTokenCounts,
    pub records: i64,
    /// The cost of the priced records, or `None` when it is unknown: when no
    /// record could be priced, and, in uploads that omit partial costs
    /// (`AiPartialBucketCost::Omitted`), when any record could not be priced.
    /// While `unpriced_records` is positive, a cost is only a known subtotal.
    pub estimated_cost_usd: Option<f64>,
    pub unpriced_records: i64,
}

/// How an upload reports the cost of a bucket that is only partly priced: some
/// of its records have a known price and some do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPartialBucketCost {
    /// Payload versions 1 and 2: the cost is `None` whenever any record is
    /// unpriced, so a partly priced bucket's known subtotal is not reported.
    Omitted,
    /// Payload version 3: the cost is the subtotal of the priced records, and
    /// `None` only when every record is unpriced.
    PricedSubtotal,
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
/// Evidence only: declared subscriptions remain authoritative. Pi's hint names
/// its ChatGPT sign-in instead, which a declared subscription to Codex pays for.
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
    snapshot_at: Option<OffsetDateTime>,
}

impl AiUsageUpload {
    pub fn new(
        machine: AiMachine,
        window: AiUsageWindow,
        pricing: AiPricing,
        mut buckets: Vec<AiUsageBucket>,
        coverage: Vec<AiProviderCoverage>,
        mut provider_hints: Vec<AiProviderHint>,
        partial_cost: AiPartialBucketCost,
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

        for (index, bucket) in buckets.iter_mut().enumerate() {
            validate_bucket(bucket, &window, &replaced, partial_cost)
                .map_err(|problem| invalid(format!("buckets[{index}]: {problem}")))?;
        }

        let mut keys = HashSet::with_capacity(buckets.len());
        for (index, bucket) in buckets.iter().enumerate() {
            let key = (
                bucket.hour_start,
                bucket.session_key.as_str(),
                bucket.project_key.as_str(),
                bucket.provider,
                bucket.model.as_str(),
                bucket.billing_provider,
            );
            if !keys.insert(key) {
                return Err(invalid(format!(
                    "buckets[{index}]: duplicate key (hourStart, sessionKey, project, provider, model, billingProvider)"
                )));
            }
        }

        let mut seen = HashSet::with_capacity(provider_hints.len());
        for (index, hint) in provider_hints.iter_mut().enumerate() {
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
            if !seen.insert((hint.provider, hint.hour_start, hint.plan.as_str())) {
                return Err(problem("duplicate hint (provider, hourStart, plan)"));
            }
        }

        Ok(Self {
            machine,
            window,
            pricing,
            buckets,
            coverage,
            provider_hints,
            snapshot_at: None,
        })
    }

    /// When the client started reading the history in this upload: every record
    /// written before it is included. Orders uploads from one machine, so an
    /// upload that read its history earlier cannot replace one that read it later.
    /// Clients that do not report it are not ordered.
    pub fn with_snapshot_at(self, snapshot_at: Option<OffsetDateTime>) -> Self {
        Self {
            snapshot_at,
            ..self
        }
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

    pub fn snapshot_at(&self) -> Option<OffsetDateTime> {
        self.snapshot_at
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
    /// The sum of the known costs only, including the priced subtotals of
    /// partly priced buckets.
    pub priced_cost_usd: f64,
    /// Buckets without a known cost. A partly priced bucket with a known
    /// subtotal is not counted here, but its unpriced records are.
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

fn validate_bucket(
    bucket: &mut AiUsageBucket,
    window: &AiUsageWindow,
    replaced: &HashSet<AiProvider>,
    partial_cost: AiPartialBucketCost,
) -> Result<(), String> {
    bucket.hour_start =
        whole_utc_hour(bucket.hour_start).ok_or("hourStart must be a whole UTC hour")?;
    if !window.contains(bucket.hour_start) {
        return Err("hourStart must be inside the window".to_string());
    }
    if !replaced.contains(&bucket.provider) {
        return Err(NOT_REPLACED.to_string());
    }
    if let Some(payer) = bucket.billing_provider {
        if payer == bucket.provider || !payer.supports_subscription() {
            return Err("billingProvider must be a different subscription provider".to_string());
        }
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
    match partial_cost {
        AiPartialBucketCost::Omitted
            if bucket.estimated_cost_usd.is_none() != (bucket.unpriced_records > 0) =>
        {
            return Err(
                "estimatedCostUsd must be null exactly when unpricedRecords is positive"
                    .to_string(),
            );
        }
        AiPartialBucketCost::PricedSubtotal
            if bucket.estimated_cost_usd.is_none()
                != (bucket.unpriced_records == bucket.records) =>
        {
            return Err(
                "estimatedCostUsd must be null exactly when every record is unpriced".to_string(),
            );
        }
        _ => {}
    }

    Ok(())
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
