//! Month close: a month's whole AI bill frozen as it was invoiced, and the
//! adjustment lines that bill each later change to it exactly once. Scope,
//! re-opening and the API are in `.docs/ai-usage-billing.md`, "Month close,
//! snapshots and adjustments".
//!
//! Everything here is pure. The billing service loads the closes, the bills
//! and the snapshots, and these functions decide which months are closed
//! (`close_blocker`), where their adjustments post (`posting_on`: the first
//! month after `K` without an active close, `next_open(K)`) and what each
//! adjustment bills. Only active closes count; a month has at most one.
//!
//! # Adjustments
//!
//! For the open month `M` and each closed `K` with `next_open(K) = M`, per
//! diff key (`AiAdjustmentKey`):
//!
//! `adjustment = R − S − F`
//!
//! - `R`: `K`'s own bill recomputed now, converted at `K`'s current rates;
//! - `S`: `K`'s snapshot, its own lines;
//! - `F`: the adjustments to `K` frozen in every active close.
//!
//! Parts that share a key are summed, and an unknown amount counts as zero.
//! The SEK amount is `R.sek − S.sek − F.sek`, not the amount converted: `S`
//! and `F` keep the SEK they were invoiced at, so a rate change alone posts
//! an SEK-only line. When nothing changed, no line posts.
//!
//! # Why every active close
//!
//! Netting only the closes before `M` double-counts once a month between `K`
//! and `M` is re-opened: with `K` and `K+1` closed (`K+1` holding nothing) and
//! `K+2` holding `A` to `K`, re-opening `K+1` makes it the first open
//! month, and no close before it holds `A`, so `A` would bill again. Netting
//! every active close is exact:
//! what is invoiced for `K` is always `S + F + A_live = R`, where `A_live` is
//! the adjustment posted live on `next_open(K)`. Closing that month moves
//! `A_live` into `F`, unchanged; re-opening a month removes its rows from `F`
//! and they reappear in `A_live`, so the identity holds through any sequence
//! of closes and re-opens, in any order. Each correction bills exactly once.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use serde_json::Value;
use time::{Date, OffsetDateTime, UtcOffset};

use super::{
    repository_key, AiBillingCharge, AiBillingLinePart, AiBillingMonth, AiBillingSubject,
    AiBillingUsage, AiCloseReadiness, AiConvertedAmount, AiConvertedBill, AiCurrency,
    AiExchangeRate, AiExchangeRateValue, AiFeeAmount, AiMappedProject, AiMeteredAccountReason,
    AiMeteredChargeKind, AiMonthBill, AiMonthOverview, AiProvider, AiProviderAccountId,
    AiProviderLogin, AiSubscriptionId, AiUnassignedKind, ProjectId, UserId,
};
use crate::domain::AiBillingError;

/// What a close freezes: the whole AI bill.
pub const AI_BILL_SCOPE: &str = "ai_bill";

/// A stored close's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AiMonthCloseId(i64);

impl AiMonthCloseId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }
}

/// One close of a month's AI bill: active until re-opened, then history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthClose {
    pub id: AiMonthCloseId,
    pub month: AiBillingMonth,
    pub closed_at: OffsetDateTime,
    /// The admin who closed it, if they still exist.
    pub closed_by: Option<String>,
    pub reopened_at: Option<OffsetDateTime>,
    pub reopened_by: Option<String>,
}

impl AiMonthClose {
    pub fn is_active(&self) -> bool {
        self.reopened_at.is_none()
    }
}

/// Whom a diff key's lines charge for, by what survives renames: an account
/// by its id, a repository by its lower-cased name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AiKeySubject {
    User(UserId),
    Account {
        provider: AiProvider,
        account_id: AiProviderAccountId,
    },
    Unknown {
        provider: AiProvider,
    },
    Repository {
        provider: AiProvider,
        key: String,
    },
}

impl AiKeySubject {
    pub fn of(subject: &AiBillingSubject) -> Self {
        match subject {
            AiBillingSubject::User(user_id) => Self::User(*user_id),
            AiBillingSubject::ProviderAccount {
                provider,
                account_id,
                ..
            } => Self::Account {
                provider: *provider,
                account_id: *account_id,
            },
            AiBillingSubject::UnknownAccount { provider } => Self::Unknown {
                provider: *provider,
            },
            AiBillingSubject::Repository { provider, name } => Self::Repository {
                provider: *provider,
                key: repository_key(name),
            },
        }
    }

    /// `user:{id}`, `account:{provider}:{id}`, `unknown:{provider}` or
    /// `repository:{provider}:{key}`.
    pub fn as_text(&self) -> String {
        match self {
            Self::User(user_id) => format!("user:{}", user_id.as_i32()),
            Self::Account {
                provider,
                account_id,
            } => format!("account:{}:{}", provider.as_str(), account_id.as_i64()),
            Self::Unknown { provider } => format!("unknown:{}", provider.as_str()),
            Self::Repository { provider, key } => {
                format!("repository:{}:{key}", provider.as_str())
            }
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let (kind, rest) = text.split_once(':')?;
        match kind {
            "user" => rest.parse().ok().map(|id| Self::User(UserId::new(id))),
            "account" => {
                let (provider, id) = rest.split_once(':')?;
                Some(Self::Account {
                    provider: AiProvider::parse(provider)?,
                    account_id: AiProviderAccountId::new(id.parse().ok()?)?,
                })
            }
            "unknown" => Some(Self::Unknown {
                provider: AiProvider::parse(rest)?,
            }),
            "repository" => {
                let (provider, key) = rest.split_once(':')?;
                let provider = AiProvider::parse(provider)?;
                (!key.is_empty()).then(|| Self::Repository {
                    provider,
                    key: key.to_string(),
                })
            }
            _ => None,
        }
    }

    /// The subject a stored row names, from its key and its label (the
    /// account's login or the repository's spelling).
    pub fn subject(&self, label: Option<&str>) -> Option<AiBillingSubject> {
        Some(match self {
            Self::User(user_id) => AiBillingSubject::User(*user_id),
            Self::Account {
                provider,
                account_id,
            } => AiBillingSubject::ProviderAccount {
                provider: *provider,
                account_id: *account_id,
                login: AiProviderLogin::parse(label?).ok()?,
            },
            Self::Unknown { provider } => AiBillingSubject::UnknownAccount {
                provider: *provider,
            },
            Self::Repository { provider, .. } => AiBillingSubject::Repository {
                provider: *provider,
                name: label?.to_string(),
            },
        })
    }
}

/// Where a diff key's lines bill: a project by its id, a kind of Unassigned,
/// or nowhere (overhead and Spinit's metered lines).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AiKeyProject {
    Project(ProjectId),
    Unassigned(AiUnassignedKind),
    None,
}

impl AiKeyProject {
    pub fn of(project: Option<&AiMappedProject>, unassigned: Option<AiUnassignedKind>) -> Self {
        match (project, unassigned) {
            (Some(project), _) => Self::Project(project.id.clone()),
            (None, Some(kind)) => Self::Unassigned(kind),
            (None, None) => Self::None,
        }
    }

    /// `project:{id}`, `unassigned:{unmapped|unmapped_repository|unattributed}`
    /// or `none`.
    pub fn as_text(&self) -> String {
        match self {
            Self::Project(id) => format!("project:{}", id.as_str()),
            Self::Unassigned(kind) => format!("unassigned:{}", unassigned_text(*kind)),
            Self::None => "none".to_string(),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        if text == "none" {
            return Some(Self::None);
        }
        if let Some(id) = text.strip_prefix("project:") {
            return (!id.is_empty()).then(|| Self::Project(ProjectId::new(id)));
        }
        let kind = match text.strip_prefix("unassigned:")? {
            "unmapped" => AiUnassignedKind::Unmapped,
            "unmapped_repository" => AiUnassignedKind::UnmappedRepository,
            "unattributed" => AiUnassignedKind::Unattributed,
            _ => return None,
        };
        Some(Self::Unassigned(kind))
    }
}

fn unassigned_text(kind: AiUnassignedKind) -> &'static str {
    match kind {
        AiUnassignedKind::Unmapped => "unmapped",
        AiUnassignedKind::UnmappedRepository => "unmapped_repository",
        AiUnassignedKind::Unattributed => "unattributed",
    }
}

/// What a diff key's lines charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiAdjustedKind {
    Api,
    Subscription,
    Overhead,
    Metered(AiMeteredChargeKind),
}

impl AiAdjustedKind {
    pub fn of(charge: &AiBillingCharge) -> Self {
        match charge {
            AiBillingCharge::Api => Self::Api,
            AiBillingCharge::Subscription { .. } => Self::Subscription,
            AiBillingCharge::UnallocatedOverhead { .. } => Self::Overhead,
            AiBillingCharge::Metered(charge) => Self::Metered(charge.kind),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Subscription => "subscription",
            Self::Overhead => "overhead",
            Self::Metered(AiMeteredChargeKind::Project) => "metered_project",
            Self::Metered(AiMeteredChargeKind::Personal) => "metered_personal",
            Self::Metered(AiMeteredChargeKind::Untracked) => "metered_untracked",
            Self::Metered(AiMeteredChargeKind::Account(AiMeteredAccountReason::Unlinked)) => {
                "metered_unlinked_account"
            }
            Self::Metered(AiMeteredChargeKind::Account(AiMeteredAccountReason::External)) => {
                "metered_external_account"
            }
            Self::Metered(AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown)) => {
                "metered_unknown_account"
            }
            Self::Metered(AiMeteredChargeKind::Repository) => "metered_repository",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "api" => Self::Api,
            "subscription" => Self::Subscription,
            "overhead" => Self::Overhead,
            "metered_project" => Self::Metered(AiMeteredChargeKind::Project),
            "metered_personal" => Self::Metered(AiMeteredChargeKind::Personal),
            "metered_untracked" => Self::Metered(AiMeteredChargeKind::Untracked),
            "metered_unlinked_account" => Self::Metered(AiMeteredChargeKind::Account(
                AiMeteredAccountReason::Unlinked,
            )),
            "metered_external_account" => Self::Metered(AiMeteredChargeKind::Account(
                AiMeteredAccountReason::External,
            )),
            "metered_unknown_account" => Self::Metered(AiMeteredChargeKind::Account(
                AiMeteredAccountReason::Unknown,
            )),
            "metered_repository" => Self::Metered(AiMeteredChargeKind::Repository),
            _ => return None,
        })
    }

    pub fn is_metered(self) -> bool {
        matches!(self, Self::Metered(_))
    }

    /// Fee kinds: their payer is a subscription.
    pub fn is_fee(self) -> bool {
        matches!(self, Self::Subscription | Self::Overhead)
    }

    /// Where the CSV ranks the key's project: projects, Unassigned, overhead,
    /// then Spinit's untracked, personal and account lines.
    fn project_rank(self, has_project: bool) -> u8 {
        match self {
            Self::Overhead => 2,
            Self::Metered(AiMeteredChargeKind::Untracked) => 3,
            Self::Metered(AiMeteredChargeKind::Personal) => 4,
            Self::Metered(AiMeteredChargeKind::Account(_)) => 5,
            _ => u8::from(!has_project),
        }
    }
}

/// Who pays for a diff key's lines: the subscription of fee kinds, else the
/// line's provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiKeyPayer {
    Provider(AiProvider),
    Subscription(AiSubscriptionId),
}

impl AiKeyPayer {
    /// `provider:{provider}` or `subscription:{id}`.
    pub fn as_text(&self) -> String {
        match self {
            Self::Provider(provider) => format!("provider:{}", provider.as_str()),
            Self::Subscription(id) => format!("subscription:{}", id.as_i32()),
        }
    }
}

/// What an adjustment nets by: lines and parts with the same key are summed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiAdjustmentKey {
    pub subject: AiKeySubject,
    pub project: AiKeyProject,
    pub kind: AiAdjustedKind,
    pub currency: AiCurrency,
    pub payer: AiKeyPayer,
}

impl AiAdjustmentKey {
    /// The key of a bill part, as the bill shows it.
    pub fn of_part(part: &AiBillingLinePart<'_>) -> Self {
        let line = part.line;
        let (currency, payer) = match &line.charge {
            AiBillingCharge::Api => (AiCurrency::usd(), AiKeyPayer::Provider(line.provider)),
            AiBillingCharge::Metered(charge) => (
                charge.amount.currency.clone(),
                AiKeyPayer::Provider(line.provider),
            ),
            AiBillingCharge::Subscription {
                subscription_id,
                fee,
            }
            | AiBillingCharge::UnallocatedOverhead {
                subscription_id,
                fee,
            } => (
                fee.currency.clone(),
                AiKeyPayer::Subscription(*subscription_id),
            ),
        };
        Self {
            subject: AiKeySubject::of(&line.subject),
            project: AiKeyProject::of(line.project.as_ref(), part.unassigned),
            kind: AiAdjustedKind::of(&line.charge),
            currency,
            payer,
        }
    }

    /// The key as its stored text columns: subject, project, kind, currency.
    /// The payer is stored as the row's provider or subscription.
    pub fn columns(&self) -> (String, String, &'static str, &str) {
        (
            self.subject.as_text(),
            self.project.as_text(),
            self.kind.as_str(),
            self.currency.as_str(),
        )
    }

    /// The subscription of fee kinds.
    pub fn subscription_id(&self) -> Option<AiSubscriptionId> {
        match self.payer {
            AiKeyPayer::Subscription(id) => Some(id),
            AiKeyPayer::Provider(_) => None,
        }
    }

    fn sort_text(&self) -> (String, String, &'static str, String, String) {
        (
            self.subject.as_text(),
            self.project.as_text(),
            self.kind.as_str(),
            self.currency.as_str().to_string(),
            self.payer.as_text(),
        )
    }
}

impl PartialOrd for AiAdjustmentKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AiAdjustmentKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_text().cmp(&other.sort_text())
    }
}

/// A part's or a stored row's amount in the billing currency, as the netting
/// counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiEntrySek {
    Known(i64),
    /// Nothing in the billing currency: its own amount is unknown, or (a
    /// stored row) it was invoiced without a rate. Counts as zero.
    Unknown,
    /// A recomputed part in a currency without a rate: the difference is
    /// unknown.
    MissingRate,
}

impl AiEntrySek {
    pub fn of(converted: &AiConvertedAmount) -> Self {
        match converted {
            AiConvertedAmount::Converted { amount, .. } => Self::Known(amount.hundredths),
            AiConvertedAmount::UnknownAmount => Self::Unknown,
            AiConvertedAmount::MissingRate { .. } => Self::MissingRate,
        }
    }

    /// What a stored row keeps: the known amount, or nothing.
    pub fn stored(self) -> Option<i64> {
        match self {
            Self::Known(hundredths) => Some(hundredths),
            Self::Unknown | Self::MissingRate => None,
        }
    }
}

/// A bill part or a snapshot row as the netting sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBilledEntry {
    pub key: AiAdjustmentKey,
    pub subject: AiBillingSubject,
    /// The line's provider as shown; for fee kinds the subscription's, so the
    /// Codex and Pi shares of a subscription name one provider.
    pub provider: AiProvider,
    pub project: Option<AiMappedProject>,
    pub unassigned: Option<AiUnassignedKind>,
    /// Hundredths of the key's currency; `None` when unknown.
    pub amount: Option<i64>,
    pub sek: AiEntrySek,
    /// The rate its amount in the billing currency was converted at; `None`
    /// in the billing currency or without a rate.
    pub rate: Option<AiExchangeRateValue>,
    /// Metered kinds' credits.
    pub credits: Option<i64>,
}

impl AiBilledEntry {
    /// A bill part's entry. `subscription_provider` names the provider of a
    /// fee kind's subscription.
    pub fn of_part(
        part: &AiBillingLinePart<'_>,
        subscription_provider: impl Fn(AiSubscriptionId) -> Option<AiProvider>,
    ) -> Result<Self, AiBillingError> {
        let key = AiAdjustmentKey::of_part(part);
        let provider = match key.subscription_id() {
            Some(id) => subscription_provider(id).ok_or_else(|| {
                AiBillingError::Invalid(format!(
                    "a line bills subscription {} that the bill does not hold",
                    id.as_i32()
                ))
            })?,
            None => part.line.provider,
        };
        let amount = part.billable.as_ref().map(|amount| amount.hundredths);
        Ok(Self {
            credits: key.kind.is_metered().then(|| amount.unwrap_or(0)),
            subject: part.line.subject.clone(),
            provider,
            project: part.line.project.clone(),
            unassigned: part.unassigned,
            amount,
            sek: AiEntrySek::of(&part.converted),
            rate: part.converted.rate().and_then(AiExchangeRate::rate),
            key,
        })
    }
}

/// A closed month whose adjustments post on an open month, and its close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdjustedMonth {
    pub month: AiBillingMonth,
    pub close_id: AiMonthCloseId,
}

/// One adjustment line on an open month's bill: a change to closed month
/// `adjusts` since it closed, for one diff key. Kept apart from the month's
/// own lines (`AiMonthBill::adjustments`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAdjustmentLine {
    pub adjusts: AiAdjustedMonth,
    pub key: AiAdjustmentKey,
    pub subject: AiBillingSubject,
    pub provider: AiProvider,
    pub project: Option<AiMappedProject>,
    pub unassigned: Option<AiUnassignedKind>,
    /// Fee kinds: the subscription's plan, while the subscription exists.
    pub plan: Option<String>,
    /// Signed, in the key's currency.
    pub amount: AiFeeAmount,
    /// The signed difference in the billing currency at the adjusted month's
    /// current rate, or `MissingRate` when it is unknown.
    pub converted: AiConvertedAmount,
    /// Metered kinds: the signed credits, equal to `amount`'s cents.
    pub credits: Option<i64>,
    /// The adjusted month's current rate, which the line carries, differs
    /// from the rate the key was last invoiced at (its latest frozen
    /// adjustment's, else its snapshot's), or the key was invoiced without
    /// one. False without a current rate, in the billing currency, and for a
    /// key never invoiced.
    pub rate_changed: bool,
}

impl AiAdjustmentLine {
    /// Only the amount in the billing currency changed: the adjusted month's
    /// exchange rate did (`rate_changed`), or, at an unchanged rate, how its
    /// SEK amounts round, such as a subscription fee's SEK split over
    /// projects whose weights moved.
    pub fn sek_only(&self) -> bool {
        self.amount.hundredths == 0
            && self
                .converted
                .amount()
                .is_some_and(|amount| amount.hundredths != 0)
    }

    /// The order an open month shows its adjustment lines in: by adjusted
    /// month, then project as the CSV ranks them, project name, Unassigned
    /// kind, subject, provider, kind and payer.
    pub fn order_key(&self) -> AdjustmentOrder {
        (
            self.adjusts.month.first_day(),
            self.key.kind.project_rank(self.project.is_some()),
            self.project
                .as_ref()
                .map(|project| (project.name.clone(), project.id.to_string())),
            self.unassigned,
            self.subject.order_key(),
            self.provider.as_str(),
            self.key.kind.as_str(),
            self.key.payer.as_text(),
        )
    }
}

/// `AiAdjustmentLine::order_key`.
pub type AdjustmentOrder = (
    Date,
    u8,
    Option<(String, String)>,
    Option<AiUnassignedKind>,
    (u8, i64, String, i64),
    &'static str,
    &'static str,
    String,
);

/// The earliest hour a reprice-only upload may change: the configured floor,
/// or `open_from`, when the first open month at or after the configured
/// floor's month starts (the earlier of its UTC and local midnight),
/// whichever is later. Buckets before it are never repriced, and every month
/// from the configured floor's month up to it is closed: the bill,
/// reconciliation and close readiness take them as they are. A closed month
/// after the first open one stays repriceable; its reprice changes only its
/// recompute, which its next open month nets as an adjustment.
pub fn reprice_floor(configured: OffsetDateTime, open_from: OffsetDateTime) -> OffsetDateTime {
    configured
        .to_offset(UtcOffset::UTC)
        .max(open_from.to_offset(UtcOffset::UTC))
}

/// The first month after `month` without an active close.
pub fn next_open(month: AiBillingMonth, active: &[AiMonthClose]) -> AiBillingMonth {
    let mut next = following(month);
    while active
        .iter()
        .any(|close| close.is_active() && close.month == next)
    {
        let after = following(next);
        if after == next {
            break;
        }
        next = after;
    }
    next
}

/// The month after `month`; `month` itself at the end of the calendar.
pub fn following(month: AiBillingMonth) -> AiBillingMonth {
    AiBillingMonth::containing(month.dates().end()).unwrap_or(month)
}

/// The closed months whose adjustments post on `target`: every active close
/// `K` with `next_open(K) = target`, oldest first. Only meaningful for an
/// open `target`.
pub fn posting_on(target: AiBillingMonth, active: &[AiMonthClose]) -> Vec<AiAdjustedMonth> {
    let mut adjusted: Vec<AiAdjustedMonth> = active
        .iter()
        .filter(|close| close.is_active() && close.month.first_day() < target.first_day())
        .filter(|close| next_open(close.month, active) == target)
        .map(|close| AiAdjustedMonth {
            month: close.month,
            close_id: close.id,
        })
        .collect();
    adjusted.sort_by_key(|month| month.month.first_day());
    adjusted
}

/// Why a month cannot close, whatever its readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCloseBlocker {
    /// The month is not over in UTC or in the billing time zone yet; it is
    /// at `over_at`.
    MonthInProgress {
        over_at: OffsetDateTime,
    },
    AlreadyClosed,
}

/// Whether `month` can close: it has no active close, and it is over both in
/// UTC (`today_utc`) and in the billing time zone (`today_local`), which is
/// at `over_at`.
pub fn close_blocker(
    month: AiBillingMonth,
    active: &[AiMonthClose],
    today_utc: Date,
    today_local: Date,
    over_at: OffsetDateTime,
) -> Option<AiCloseBlocker> {
    if active
        .iter()
        .any(|close| close.is_active() && close.month == month)
    {
        return Some(AiCloseBlocker::AlreadyClosed);
    }
    if today_utc <= month.last_day() || today_local <= month.last_day() {
        return Some(AiCloseBlocker::MonthInProgress { over_at });
    }
    None
}

/// Sums of one diff key over the three sources.
#[derive(Default)]
struct KeySums<'a> {
    recompute: i64,
    snapshot: i64,
    frozen: i64,
    recompute_sek: i64,
    snapshot_sek: i64,
    frozen_sek: i64,
    missing_rate: bool,
    /// The rate (or none) of the latest stored entry with a known amount:
    /// snapshot rows, then frozen adjustments in close order.
    invoiced_rate: Option<Option<AiExchangeRateValue>>,
    /// The latest source's entry: recompute, then snapshot, then frozen.
    display: Option<(u8, &'a AiBilledEntry)>,
}

/// The adjustment lines of closed month `adjusted.month` on the open month
/// they post on: per diff key, `recompute − snapshot − frozen`, in amount and
/// in the billing currency (see the module documentation). `rates` are the
/// adjusted month's current rates, which the lines carry; `plans` name the
/// subscriptions that still exist. `frozen` is in close order, oldest first,
/// so a line's `rate_changed` compares with the rate last invoiced. Lines are
/// in `AiAdjustmentLine::order_key` order.
pub fn adjustment_lines(
    adjusted: AiAdjustedMonth,
    recompute: &[AiBilledEntry],
    snapshot: &[AiBilledEntry],
    frozen: &[AiBilledEntry],
    rates: &[AiExchangeRate],
    plans: &HashMap<AiSubscriptionId, String>,
) -> Result<Vec<AiAdjustmentLine>, AiBillingError> {
    fn add(sum: &mut i64, value: Option<i64>) -> Result<(), AiBillingError> {
        *sum = sum
            .checked_add(value.unwrap_or(0))
            .ok_or(AiBillingError::NumericRange)?;
        Ok(())
    }

    let mut keys: BTreeMap<&AiAdjustmentKey, KeySums<'_>> = BTreeMap::new();
    for (rank, entries) in [(0_u8, recompute), (1, snapshot), (2, frozen)] {
        for entry in entries {
            let sums = keys.entry(&entry.key).or_default();
            let (amount, sek) = match rank {
                0 => (&mut sums.recompute, &mut sums.recompute_sek),
                1 => (&mut sums.snapshot, &mut sums.snapshot_sek),
                _ => (&mut sums.frozen, &mut sums.frozen_sek),
            };
            add(amount, entry.amount)?;
            match entry.sek {
                AiEntrySek::Known(hundredths) => add(sek, Some(hundredths))?,
                AiEntrySek::Unknown => {}
                AiEntrySek::MissingRate => {
                    if rank == 0 {
                        sums.missing_rate = true;
                    }
                }
            }
            if rank > 0 && entry.amount.is_some() {
                sums.invoiced_rate = Some(entry.rate);
            }
            if sums.display.is_none_or(|(shown, _)| shown > rank) {
                sums.display = Some((rank, entry));
            }
        }
    }

    let difference = |r: i64, s: i64, f: i64| {
        r.checked_sub(s)
            .and_then(|value| value.checked_sub(f))
            .ok_or(AiBillingError::NumericRange)
    };
    let billing = AiCurrency::billing();
    let mut lines = Vec::new();
    for (key, sums) in keys {
        let amount = difference(sums.recompute, sums.snapshot, sums.frozen)?;
        let sek = difference(sums.recompute_sek, sums.snapshot_sek, sums.frozen_sek)?;
        if amount == 0 && sek == 0 && !sums.missing_rate {
            continue;
        }
        let Some((_, entry)) = sums.display else {
            continue;
        };
        let converted = if sums.missing_rate {
            AiConvertedAmount::MissingRate {
                currency: key.currency.clone(),
            }
        } else {
            AiConvertedAmount::Converted {
                amount: AiFeeAmount {
                    hundredths: sek,
                    currency: billing.clone(),
                },
                rate: (key.currency != billing)
                    .then(|| rates.iter().find(|rate| rate.currency == key.currency))
                    .flatten()
                    .filter(|rate| rate.rate().is_some())
                    .cloned(),
            }
        };
        let current_rate = converted.rate().and_then(AiExchangeRate::rate);
        let rate_changed = matches!(
            (current_rate, sums.invoiced_rate),
            (Some(current), Some(invoiced)) if invoiced != Some(current)
        );
        lines.push(AiAdjustmentLine {
            adjusts: adjusted,
            key: key.clone(),
            subject: entry.subject.clone(),
            provider: entry.provider,
            project: entry.project.clone(),
            unassigned: entry.unassigned,
            plan: key.subscription_id().and_then(|id| plans.get(&id).cloned()),
            amount: AiFeeAmount {
                hundredths: amount,
                currency: key.currency.clone(),
            },
            converted,
            credits: key.kind.is_metered().then_some(amount),
            rate_changed,
        });
    }
    lines.sort_by_cached_key(AiAdjustmentLine::order_key);
    Ok(lines)
}

/// The shape of a stored projection (`AiStoredSnapshot`): a change to the
/// served overview, line, adjustment line or developer JSON bumps it, with
/// an upgrade step from the previous version where the snapshot is served.
pub const AI_BILL_PROJECTION_VERSION: i16 = 1;

/// A close about to be stored, with its snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct AiNewClose {
    /// Reserved beforehand, so the snapshot's CSV can name it.
    pub id: AiMonthCloseId,
    pub month: AiBillingMonth,
    pub by: UserId,
    pub readiness_json: Value,
    pub bill_json: Value,
    pub developer_json: Value,
    pub csv_header: Vec<String>,
    pub projection_version: i16,
    /// Own lines first, then adjustment lines, by `position`.
    pub lines: Vec<AiNewSnapshotLine>,
}

/// One snapshot row: a bill part or an adjustment line as its readers saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiNewSnapshotLine {
    pub position: u32,
    pub csv_position: u32,
    /// An adjustment line's corrected month and its close; `None` on own lines.
    pub adjusts: Option<AiAdjustedMonth>,
    /// Its key, amounts and the rate its amount in the billing currency used.
    pub entry: AiBilledEntry,
    pub line_json: Value,
    /// Raw cell values, guarded when served.
    pub csv_row: Vec<String>,
}

/// What a closed month serves.
#[derive(Debug, Clone, PartialEq)]
pub struct AiStoredSnapshot {
    pub bill_json: Value,
    pub developer_json: Value,
    pub readiness_json: Value,
    pub csv_header: Vec<String>,
    pub projection_version: i16,
    /// By `position`: own lines, then adjustment lines.
    pub lines: Vec<AiStoredSnapshotLine>,
}

/// One stored snapshot row as a closed month serves it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiStoredSnapshotLine {
    pub csv_position: u32,
    /// An adjustment line's corrected month; `None` on own lines.
    pub adjusts: Option<AiBillingMonth>,
    pub subject: AiKeySubject,
    pub line_json: Value,
    pub csv_row: Vec<String>,
}

/// A closed month: its close, where its adjustments post, its snapshot and
/// its rates as stored now (an override after close changes them, not the
/// snapshot).
#[derive(Debug, Clone, PartialEq)]
pub struct AiClosedMonth {
    pub close: AiMonthClose,
    pub adjustments_post_on: AiBillingMonth,
    pub snapshot: AiStoredSnapshot,
    pub current_rates: Vec<AiExchangeRate>,
}

/// A month's bill: live while it is open, frozen once it closed.
#[derive(Debug, Clone, PartialEq)]
pub enum AiMonthView {
    Open(Box<AiMonthOverview>),
    Closed(Box<AiClosedMonth>),
}

/// A developer's bill in a month.
#[derive(Debug, Clone, PartialEq)]
pub enum AiDeveloperBill {
    /// An open month: the developer's lines, subscriptions and adjustment
    /// lines, converted, and their usage of the keys they marked personal,
    /// which the bill and the usage by day leave out (`None` without any).
    Live {
        bill: Box<AiMonthBill>,
        converted: Box<AiConvertedBill>,
        personal: Option<AiBillingUsage>,
    },
    /// A closed month, from its snapshot.
    Closed(Box<AiClosedDeveloperBill>),
}

/// A developer's bill in a closed month, from its snapshot: the overview's
/// bill part (period, rates, and the personal usage of everyone, of which
/// the page shows the developer's), the developer's bill part (`developer`:
/// subscriptions, totals, metered; `None` without any), and their own and
/// adjustment lines. A key marked personal after close changes none of it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiClosedDeveloperBill {
    pub close: AiMonthClose,
    pub adjustments_post_on: AiBillingMonth,
    pub current_rates: Vec<AiExchangeRate>,
    pub bill_json: Value,
    pub projection_version: i16,
    pub developer: Option<Value>,
    pub lines: Vec<Value>,
    pub adjustment_lines: Vec<Value>,
}

/// Where a month stands with close.
#[derive(Debug, Clone, PartialEq)]
pub enum AiMonthCloseState {
    Open {
        /// When the month is over in UTC and the billing time zone; `None`
        /// once it is.
        over_at: Option<OffsetDateTime>,
        /// Evaluated once the month is over.
        readiness: Option<AiCloseReadiness>,
        /// The closed months whose adjustments post on this one.
        adjusts: Vec<AiAdjustedMonth>,
    },
    Closed {
        close: AiMonthClose,
        readiness_json: Value,
        adjustments_post_on: AiBillingMonth,
        /// Months whose active closes hold adjustments to this one: it cannot
        /// be re-opened before they are.
        held_by: Vec<AiBillingMonth>,
    },
}

/// A month's close state and its close history.
#[derive(Debug, Clone, PartialEq)]
pub struct AiMonthCloseStatus {
    pub month: AiBillingMonth,
    pub state: AiMonthCloseState,
    /// Every close of the month, re-opened ones included, newest first.
    pub history: Vec<AiMonthClose>,
}

#[cfg(test)]
mod tests;
