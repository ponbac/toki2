//! What a metered payer billed, as its usage source reports it: credits per
//! account (or organization-wide) per product, SKU and model label, the
//! organization's rows per SKU and repository, and the month's summary.
//!
//! For GitHub Copilot a credit is worth $0.01 at list. `G`, an account's gross
//! credits in dollars, is the payer's gross quantity × $0.01, rounded half up
//! to nanodollars per row (`AiDecimal::credits_to_usd_nanos`); the payer's own
//! gross amount is kept as given but is not exactly that product.
//!
//! Numbers are kept exactly as the payer wrote them (`AiDecimal`), so sums and
//! comparisons with the payer's month totals need no floating point.

use std::{cmp::Ordering, collections::BTreeMap, fmt};

use time::{Date, OffsetDateTime};

use super::{AiBillingMonth, AiProvider, AiProviderAccountId, AiProviderLogin, AiUsdNanos};

/// Nanodollars per credit: one credit is $0.01.
pub const NANOS_PER_CREDIT: i64 = 10_000_000;
/// The most decimals an `AiDecimal` keeps.
pub const MAX_DECIMAL_SCALE: u32 = 36;
/// The most significant digits an `AiDecimal` parses.
const MAX_SIGNIFICANT_DIGITS: usize = 30;
/// `log10(NANOS_PER_CREDIT)`.
const CREDIT_NANOS_SCALE: u32 = 7;
/// Two quantities match when they differ by at most 10^-6.
const MATCH_TOLERANCE_DECIMALS: u32 = 6;

/// A non-negative decimal exactly as a payer reported it, such as a credit
/// quantity or an amount in dollars, float artifacts such as
/// `0.0034338949999999993` included. Kept canonical (no trailing zeros), so
/// equality is equality of value; at most `MAX_DECIMAL_SCALE` decimals. Sums
/// are exact or `None`; comparisons are always exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiDecimal {
    unscaled: i128,
    scale: u32,
}

fn pow10(exponent: u32) -> i128 {
    10_i128.pow(exponent)
}

impl AiDecimal {
    pub const ZERO: Self = Self {
        unscaled: 0,
        scale: 0,
    };

    /// A canonical decimal from `unscaled × 10^-scale`, if it is not negative
    /// and has at most `MAX_DECIMAL_SCALE` decimals once canonical.
    fn new(mut unscaled: i128, mut scale: u32) -> Option<Self> {
        if unscaled < 0 {
            return None;
        }
        while scale > 0 && unscaled % 10 == 0 {
            unscaled /= 10;
            scale -= 1;
        }
        if unscaled == 0 {
            return Some(Self::ZERO);
        }
        (scale <= MAX_DECIMAL_SCALE).then_some(Self { unscaled, scale })
    }

    /// The whole part and the fraction at `scale` (at least this value's).
    fn split(&self, scale: u32) -> (i128, i128) {
        let divisor = pow10(self.scale);
        (
            self.unscaled / divisor,
            (self.unscaled % divisor) * pow10(scale - self.scale),
        )
    }

    /// Parses a plain decimal literal, `\d+(\.\d+)?([eE][+-]?\d+)?`, such as
    /// `12.5`, `19.0` or `3.12e-05`, exactly. No sign, no whitespace, at most
    /// 30 significant digits and, after the exponent, at most 18 decimals.
    pub fn parse(raw: &str) -> Option<Self> {
        let (mantissa, exponent) = match raw.find(['e', 'E']) {
            Some(at) => (&raw[..at], Some(&raw[at + 1..])),
            None => (raw, None),
        };
        let (integer, fraction) = match mantissa.split_once('.') {
            Some((integer, fraction)) => (integer, fraction),
            None => (mantissa, ""),
        };
        let all_digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        if !all_digits(integer) || (mantissa.contains('.') && !all_digits(fraction)) {
            return None;
        }
        let exponent: i64 = match exponent {
            None => 0,
            Some(text) => {
                let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
                if !all_digits(digits) || digits.len() > 4 {
                    return None;
                }
                let value: i64 = digits.parse().ok()?;
                if text.starts_with('-') {
                    -value
                } else {
                    value
                }
            }
        };

        let digits = format!("{integer}{fraction}");
        let significant = digits.trim_start_matches('0');
        let trailing_zeros = significant.len() - significant.trim_end_matches('0').len();
        let significant = significant.trim_end_matches('0');
        if significant.is_empty() {
            return Some(Self::ZERO);
        }
        if significant.len() > MAX_SIGNIFICANT_DIGITS {
            return None;
        }
        let unscaled: i128 = significant.parse().ok()?;
        let scale = fraction.len() as i64 - exponent - trailing_zeros as i64;
        if scale < 0 {
            let factor = u32::try_from(-scale).ok().filter(|f| *f <= 38)?;
            Self::new(unscaled.checked_mul(pow10(factor))?, 0)
        } else {
            Self::new(unscaled, u32::try_from(scale).ok()?)
        }
    }

    /// The value of these many credits in nanodollars, at $0.01 a credit:
    /// `self × 10^7`, rounded half up. `None` beyond `i64`.
    pub fn credits_to_usd_nanos(&self) -> Option<i64> {
        let nanos = if self.scale <= CREDIT_NANOS_SCALE {
            self.unscaled
                .checked_mul(pow10(CREDIT_NANOS_SCALE - self.scale))?
        } else {
            let divisor = pow10(self.scale - CREDIT_NANOS_SCALE);
            let quotient = self.unscaled / divisor;
            let remainder = self.unscaled % divisor;
            quotient + i128::from(remainder * 2 >= divisor)
        };
        i64::try_from(nanos).ok()
    }

    /// The exact sum, or `None` when it is no longer representable.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        let scale = self.scale.max(other.scale);
        let left = self.unscaled.checked_mul(pow10(scale - self.scale))?;
        let right = other.unscaled.checked_mul(pow10(scale - other.scale))?;
        Self::new(left.checked_add(right)?, scale)
    }

    /// The exact sum of `values`, or `None` when it is not representable.
    pub fn sum<'a>(values: impl IntoIterator<Item = &'a Self>) -> Option<Self> {
        values
            .into_iter()
            .try_fold(Self::ZERO, |total, value| total.checked_add(value))
    }

    /// Whether the two differ by at most 0.000001.
    pub fn matches(&self, other: &Self) -> bool {
        let scale = self.scale.max(other.scale).max(MATCH_TOLERANCE_DECIMALS);
        let (whole_a, fraction_a) = self.split(scale);
        let (whole_b, fraction_b) = other.split(scale);
        if (whole_a - whole_b).abs() > 1 {
            return false;
        }
        // Fractions are below 10^scale ≤ 10^36, so this fits.
        let difference = (whole_a - whole_b) * pow10(scale) + (fraction_a - fraction_b);
        difference.abs() <= pow10(scale - MATCH_TOLERANCE_DECIMALS)
    }

    pub fn is_zero(&self) -> bool {
        self.unscaled == 0
    }

    /// The value with exactly `decimals` decimals, rounded half up, such as
    /// `12.35` for 12.345 at 2.
    pub fn to_fixed(self, decimals: u32) -> String {
        let decimals = decimals.min(MAX_DECIMAL_SCALE);
        if self.scale <= decimals {
            // Exact: pad the canonical form with zeros.
            let shown = self.to_string();
            let (whole, fraction) = shown.split_once('.').unwrap_or((&shown, ""));
            if decimals == 0 {
                return whole.to_string();
            }
            return format!("{whole}.{fraction:0<width$}", width = decimals as usize);
        }
        let divisor = pow10(self.scale - decimals);
        let rounded = self.unscaled / divisor + i128::from(self.unscaled % divisor * 2 >= divisor);
        if decimals == 0 {
            return rounded.to_string();
        }
        let unit = pow10(decimals);
        format!(
            "{}.{:0width$}",
            rounded / unit,
            rounded % unit,
            width = decimals as usize
        )
    }
}

impl Default for AiDecimal {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Ord for AiDecimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let scale = self.scale.max(other.scale);
        self.split(scale).cmp(&other.split(scale))
    }
}

impl PartialOrd for AiDecimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for AiDecimal {
    /// Canonical, never with an exponent: `12.5`, `0.0000312`, `0`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(f, "{}", self.unscaled);
        }
        let divisor = pow10(self.scale);
        write!(
            f,
            "{}.{:0width$}",
            self.unscaled / divisor,
            self.unscaled % divisor,
            width = self.scale as usize
        )
    }
}

/// How a credits row is billed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiCreditKind {
    /// Credits of a model Toki tracks: reconciled against Toki's usage.
    Trackable,
    /// Credits Toki cannot track, such as the cloud agent's: untracked.
    Untrackable,
    /// Credits the payer also bills per repository, such as code review:
    /// never reconciled per account, so never billed twice.
    RepositoryBilled,
    /// Another SKU, unit or unit price: kept and flagged, never trackable.
    Unrecognized,
}

impl AiCreditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trackable => "trackable",
            Self::Untrackable => "untrackable",
            Self::RepositoryBilled => "repository_billed",
            Self::Unrecognized => "unrecognized",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "trackable" => Some(Self::Trackable),
            "untrackable" => Some(Self::Untrackable),
            "repository_billed" => Some(Self::RepositoryBilled),
            "unrecognized" => Some(Self::Unrecognized),
            _ => None,
        }
    }
}

/// What an organization usage row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiOrgChargeKind {
    /// Credits billed to a named repository, such as code review.
    RepositoryCredits,
    /// Credits without a repository: the sum of every account's credits (the
    /// IDE aggregate). Never added to per-account rows.
    AccountCreditsAggregate,
    /// Credits Toki cannot track, such as the cloud agent's.
    UntrackableCredits,
    /// Compute such as Actions minutes and storage: never billed on.
    Compute,
    /// Seat fees.
    Seats,
    Unrecognized,
}

impl AiOrgChargeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryCredits => "repository_credits",
            Self::AccountCreditsAggregate => "account_credits_aggregate",
            Self::UntrackableCredits => "untrackable_credits",
            Self::Compute => "compute",
            Self::Seats => "seats",
            Self::Unrecognized => "unrecognized",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "repository_credits" => Some(Self::RepositoryCredits),
            "account_credits_aggregate" => Some(Self::AccountCreditsAggregate),
            "untrackable_credits" => Some(Self::UntrackableCredits),
            "compute" => Some(Self::Compute),
            "seats" => Some(Self::Seats),
            "unrecognized" => Some(Self::Unrecognized),
            _ => None,
        }
    }

    /// Whether the row's quantity is credits at $0.01.
    pub fn is_credits(self) -> bool {
        match self {
            Self::RepositoryCredits | Self::AccountCreditsAggregate | Self::UntrackableCredits => {
                true
            }
            Self::Compute | Self::Seats | Self::Unrecognized => false,
        }
    }
}

/// What a month summary row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiSummaryKind {
    /// Credits Toki tracks, with the pool discount.
    Credits,
    /// Credits Toki cannot track, such as the cloud agent's.
    UntrackableCredits,
    Seats,
    Unrecognized,
}

impl AiSummaryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Credits => "credits",
            Self::UntrackableCredits => "untrackable_credits",
            Self::Seats => "seats",
            Self::Unrecognized => "unrecognized",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "credits" => Some(Self::Credits),
            "untrackable_credits" => Some(Self::UntrackableCredits),
            "seats" => Some(Self::Seats),
            "unrecognized" => Some(Self::Unrecognized),
            _ => None,
        }
    }
}

/// A payer's model label, normalized by the payer's adapter to the model
/// naming of Toki's usage buckets, such as `GPT-6.1 Sol` → `gpt-6.1-sol`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiMeteredModel {
    /// The label as the payer gave it.
    pub label: String,
    /// In Toki's naming; empty when the label is.
    pub model: String,
    /// Whether the payer routed the usage automatically ("Auto: X" counts as
    /// X, already discounted).
    pub auto_routed: bool,
}

/// Quantities and amounts exactly as the payer gave them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiMeteredAmounts {
    pub unit_price: AiDecimal,
    pub gross_quantity: AiDecimal,
    pub gross_amount: AiDecimal,
    pub discount_quantity: AiDecimal,
    pub discount_amount: AiDecimal,
    pub net_quantity: AiDecimal,
    pub net_amount: AiDecimal,
}

impl AiMeteredAmounts {
    /// The sums of everything but the unit price, which is kept.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        Some(Self {
            unit_price: self.unit_price,
            gross_quantity: self.gross_quantity.checked_add(&other.gross_quantity)?,
            gross_amount: self.gross_amount.checked_add(&other.gross_amount)?,
            discount_quantity: self
                .discount_quantity
                .checked_add(&other.discount_quantity)?,
            discount_amount: self.discount_amount.checked_add(&other.discount_amount)?,
            net_quantity: self.net_quantity.checked_add(&other.net_quantity)?,
            net_amount: self.net_amount.checked_add(&other.net_amount)?,
        })
    }
}

/// What a credits row is keyed by: product, SKU and the payer's label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AiCreditKey {
    pub product: String,
    pub sku: String,
    pub label: String,
}

/// One credits row of an account, or organization-wide per model, for a day
/// or a month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredCredit {
    pub product: String,
    pub sku: String,
    /// The payer's unit, such as `ai-credits`.
    pub unit: String,
    pub model: AiMeteredModel,
    pub kind: AiCreditKind,
    pub amounts: AiMeteredAmounts,
}

impl AiMeteredCredit {
    /// `G`: the gross quantity × $0.01, rounded half up to nanodollars.
    pub fn gross_usd(&self) -> Option<AiUsdNanos> {
        self.amounts
            .gross_quantity
            .credits_to_usd_nanos()
            .and_then(AiUsdNanos::new)
    }

    pub fn key(&self) -> AiCreditKey {
        AiCreditKey {
            product: self.product.clone(),
            sku: self.sku.clone(),
            label: self.model.label.clone(),
        }
    }
}

/// One organization usage row, on its UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredOrgCharge {
    pub day: Date,
    pub product: String,
    pub sku: String,
    pub unit: String,
    /// The bare repository name; empty when the row names none.
    pub repository: String,
    pub kind: AiOrgChargeKind,
    pub unit_price: AiDecimal,
    pub quantity: AiDecimal,
    pub gross_amount: AiDecimal,
    pub discount_amount: AiDecimal,
    pub net_amount: AiDecimal,
    /// The instant the payer gave the row, if any.
    pub reported_at: Option<OffsetDateTime>,
}

impl AiMeteredOrgCharge {
    /// The quantity × $0.01 for credit rows, rounded half up; `None` for other
    /// kinds, or beyond `i64` nanodollars.
    pub fn credit_usd(&self) -> Option<AiUsdNanos> {
        if !self.kind.is_credits() {
            return None;
        }
        self.quantity
            .credits_to_usd_nanos()
            .and_then(AiUsdNanos::new)
    }
}

/// One row of a month's summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredSummaryLine {
    pub product: String,
    pub sku: String,
    pub unit: String,
    pub kind: AiSummaryKind,
    pub amounts: AiMeteredAmounts,
}

/// Whose credits a usage source is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMeteredScope<'a> {
    Account(&'a AiProviderLogin),
    Organization,
}

/// Whose credits are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiStoredScope {
    Account(AiProviderAccountId),
    Organization,
}

impl AiStoredScope {
    pub fn account_id(self) -> Option<AiProviderAccountId> {
        match self {
            Self::Account(account_id) => Some(account_id),
            Self::Organization => None,
        }
    }
}

/// Whose credits are written, with the login an account's were requested
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCreditStoreScope<'a> {
    Account {
        id: AiProviderAccountId,
        login: &'a AiProviderLogin,
    },
    Organization,
}

impl AiCreditStoreScope<'_> {
    pub fn stored(&self) -> AiStoredScope {
        match self {
            Self::Account { id, .. } => AiStoredScope::Account(*id),
            Self::Organization => AiStoredScope::Organization,
        }
    }
}

/// The period a usage source is asked for: a UTC month, summed, or one UTC
/// day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMeteredPeriod {
    Month(AiBillingMonth),
    Day(Date),
}

/// What an import reads from a usage source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiMeteredSource {
    /// Each account's credits per UTC day.
    AccountCredits,
    /// The organization's credits per UTC day, for the residual.
    OrgCredits,
    /// The organization's rows per SKU and repository per UTC day.
    OrgUsage,
    /// Each month's summary.
    Summary,
}

impl AiMeteredSource {
    pub const ALL: [Self; 4] = [
        Self::AccountCredits,
        Self::OrgCredits,
        Self::OrgUsage,
        Self::Summary,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountCredits => "account_credits",
            Self::OrgCredits => "org_credits",
            Self::OrgUsage => "org_usage",
            Self::Summary => "summary",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "account_credits" => Some(Self::AccountCredits),
            "org_credits" => Some(Self::OrgCredits),
            "org_usage" => Some(Self::OrgUsage),
            "summary" => Some(Self::Summary),
            _ => None,
        }
    }

    /// How people read it.
    pub fn label(self) -> &'static str {
        match self {
            Self::AccountCredits => "account credits",
            Self::OrgCredits => "organization credits",
            Self::OrgUsage => "organization usage",
            Self::Summary => "the month summary",
        }
    }
}

/// A key's gross quantity summed over rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCreditTotal {
    pub key: AiCreditKey,
    pub gross_quantity: AiDecimal,
}

/// The gross quantity per key, by key. `None` when a sum is not
/// representable.
pub fn credit_totals(rows: &[AiMeteredCredit]) -> Option<Vec<AiCreditTotal>> {
    let mut totals: BTreeMap<AiCreditKey, AiDecimal> = BTreeMap::new();
    for row in rows {
        let total = totals.entry(row.key()).or_default();
        *total = total.checked_add(&row.amounts.gross_quantity)?;
    }
    Some(
        totals
            .into_iter()
            .map(|(key, gross_quantity)| AiCreditTotal {
                key,
                gross_quantity,
            })
            .collect(),
    )
}

/// The sum of every key's gross quantity.
pub fn total_gross_quantity(totals: &[AiCreditTotal]) -> Option<AiDecimal> {
    AiDecimal::sum(totals.iter().map(|total| &total.gross_quantity))
}

/// Whether every key's totals `matches`; a key missing on one side counts as
/// zero.
pub fn credit_totals_match(payer: &[AiCreditTotal], stored: &[AiCreditTotal]) -> bool {
    let mut pairs: BTreeMap<&AiCreditKey, (AiDecimal, AiDecimal)> = BTreeMap::new();
    for total in payer {
        pairs.entry(&total.key).or_default().0 = total.gross_quantity;
    }
    for total in stored {
        pairs.entry(&total.key).or_default().1 = total.gross_quantity;
    }
    pairs.values().all(|(payer, stored)| payer.matches(stored))
}

/// Sums rows that share a key. An error naming the key when such rows differ
/// in unit, unit price, model or kind, or a sum is not representable.
pub fn merge_credits(rows: Vec<AiMeteredCredit>) -> Result<Vec<AiMeteredCredit>, String> {
    let mut merged: BTreeMap<AiCreditKey, AiMeteredCredit> = BTreeMap::new();
    for row in rows {
        let key = row.key();
        match merged.get_mut(&key) {
            None => {
                merged.insert(key, row);
            }
            Some(existing) => {
                let name = || format!("{} / {} / {:?}", key.product, key.sku, key.label);
                if existing.unit != row.unit
                    || existing.amounts.unit_price != row.amounts.unit_price
                    || existing.model != row.model
                    || existing.kind != row.kind
                {
                    return Err(format!(
                        "two rows for {} differ in unit or unit price",
                        name()
                    ));
                }
                existing.amounts = existing
                    .amounts
                    .checked_add(&row.amounts)
                    .ok_or_else(|| format!("the rows for {} sum out of range", name()))?;
            }
        }
    }
    Ok(merged.into_values().collect())
}

/// Sums organization rows that share a day, product, SKU, unit and
/// repository; the earliest reported instant is kept. An error naming the key
/// when such rows differ in unit price or kind.
pub fn merge_org_charges(rows: Vec<AiMeteredOrgCharge>) -> Result<Vec<AiMeteredOrgCharge>, String> {
    type Key = (Date, String, String, String, String);
    let mut merged: BTreeMap<Key, AiMeteredOrgCharge> = BTreeMap::new();
    for row in rows {
        let key: Key = (
            row.day,
            row.product.clone(),
            row.sku.clone(),
            row.unit.clone(),
            row.repository.clone(),
        );
        match merged.get_mut(&key) {
            None => {
                merged.insert(key, row);
            }
            Some(existing) => {
                let name = || {
                    format!(
                        "{} {} / {} / {:?} / {:?}",
                        key.0, key.1, key.2, key.3, key.4
                    )
                };
                if existing.unit_price != row.unit_price || existing.kind != row.kind {
                    return Err(format!("two rows for {} differ in unit price", name()));
                }
                let sum = |a: &AiDecimal, b: &AiDecimal| {
                    a.checked_add(b)
                        .ok_or_else(|| format!("the rows for {} sum out of range", name()))
                };
                existing.quantity = sum(&existing.quantity, &row.quantity)?;
                existing.gross_amount = sum(&existing.gross_amount, &row.gross_amount)?;
                existing.discount_amount = sum(&existing.discount_amount, &row.discount_amount)?;
                existing.net_amount = sum(&existing.net_amount, &row.net_amount)?;
                existing.reported_at = match (existing.reported_at, row.reported_at) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
        }
    }
    Ok(merged.into_values().collect())
}

/// Whether a metered payer bills a model, in Toki's naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiMeteredModelStatus {
    /// Billed, with billing evidence for its rates.
    Confirmed,
    /// Billed, at rates without billing evidence yet.
    Provisional,
    /// Not billed: such usage carries no weight.
    Free,
}

impl AiMeteredModelStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Provisional => "provisional",
            Self::Free => "free",
        }
    }
}

/// Copilot's billed models, in Toki's naming. Mirrors Token Ledger's
/// `copilot-rates.json` (the card of 2026-10-24); keep the two in sync.
const COPILOT_CONFIRMED: &[&str] = &[
    "gpt-6.1-sol",
    "gpt-6-sol",
    "gpt-6-luna",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.3-codex",
    "gpt-5.2",
    "gpt-5.4-mini",
    "claude-sonnet-5.5",
    "claude-haiku-5.5",
    "mai-code-1.1-flash",
];
const COPILOT_PROVISIONAL: &[&str] = &[
    "gpt-6-astra",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "claude-opus-5.5",
    "claude-opus-5",
    "claude-opus-4.8",
];
/// Copilot's free models, matched as prefixes: next edit suggestions and the
/// helper model.
const COPILOT_FREE_PREFIXES: &[&str] = &["copilot-nes-", "nes-", "gpt-4o-mini-"];
const COPILOT_FREE: &[&str] = &["gpt-4o-mini"];

/// The status of `model` at a metered payer, or `None` when the payer's
/// catalog does not know it (or the payer is not metered).
pub fn metered_model_status(payer: AiProvider, model: &str) -> Option<AiMeteredModelStatus> {
    match payer {
        AiProvider::Copilot => {
            if COPILOT_CONFIRMED.contains(&model) {
                Some(AiMeteredModelStatus::Confirmed)
            } else if COPILOT_PROVISIONAL.contains(&model) {
                Some(AiMeteredModelStatus::Provisional)
            } else if COPILOT_FREE.contains(&model)
                || COPILOT_FREE_PREFIXES
                    .iter()
                    .any(|prefix| model.starts_with(prefix))
            {
                Some(AiMeteredModelStatus::Free)
            } else {
                None
            }
        }
        AiProvider::Codex
        | AiProvider::Claude
        | AiProvider::Grok
        | AiProvider::Pi
        | AiProvider::OpenCode => None,
    }
}

#[cfg(test)]
mod tests;
