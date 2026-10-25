//! Reconciliation of what a metered payer billed (`G`, gross credits) with
//! what Toki tracked (`T`, Toki's estimate of the usage the payer paid for, at
//! the payer's price), per account, UTC day and group of (SKU, model). From
//! the metered cutover on, the bill's lines follow this attribution: the bill
//! allocates over the same walk of the month (`walk`, `split_group`), so the
//! two agree on `G`, `T`, attributed and untracked for every account
//! (`ai_metered_billing`).
//!
//! - **Owners.** Each account day belongs to the account's disposition that
//!   day: a user (linked), External, or nobody (unlinked).
//! - **Groups.** On linked days, credits form groups of (SKU, normalized
//!   model). A trackable group's `T` is the user's priced tracked cost of that
//!   model that day, per destination (`AiMeteredDestination`: the project its
//!   key resolves to, a kind of Unassigned, or personal/internal); untrackable
//!   and unrecognized groups have `T = 0`; repository-billed credits (code
//!   review) are left out, since the payer bills them per repository. A
//!   tracked model without credits that day forms a group with Toki usage only
//!   (`G = 0`).
//! - **The rule** (`split_group`). Per day and group, `attributed = min(G, T)`
//!   and `untracked = G − attributed`: `T ≥ G` splits `G` by each
//!   destination's `T_d`, `T < G` gives each its `T_d`. No destination gets
//!   more than its `T_d`. Unpriced usage carries no weight, so a group with
//!   only unpriced usage has `T = 0`. Free models carry none either: their
//!   tracked cost is left out of `T` everywhere (old pricing basis and tracked
//!   without a linked account included), and their groups are listed with
//!   `T = 0`.
//! - **Imported days.** An account day whose slice was never fetched has no
//!   `G` to compare with: it is counted as not imported and its `T` is left
//!   out, rather than read as the payer billing nothing.
//! - **Flags** that compare `G` with `T` weigh settled days only: the
//!   account's slice is complete and the day is before the settle days, as
//!   for the residual. Days the payer may still post to never raise them.
//!   Billed nothing needs material `T` over the month; `G/T` below 0.85 is
//!   raised by any settled day-group, whatever its size.
//! - **Elsewhere.** On unlinked and External days every non-repository credit
//!   is `unlinked` or `external`. A user's `T` on days none of their
//!   dispositions covers is `unlinked_tracked`.
//! - **Identity.** Per account, `gross = repository_billed + attributed +
//!   untracked + unlinked + external`.
//! - **Residual.** Per UTC day, the organization's credits minus every
//!   account's: credits of accounts Toki never listed ("unknown account").
//!   It rests on the organization's stored days, so they must equal the
//!   payer's month total: until the organization's check matches, no day
//!   settles, and a failed check makes every day an import gap. Days with a
//!   missing slice are an import gap too, and days that may still change are
//!   provisional.
//!
//! Amounts are nanodollars (a credit is 10^7). Signed figures (residuals) are
//! plain `i64` nanodollars.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use time::{Date, Duration, OffsetDateTime};

use super::{
    allocate_largest_remainder, metered_model_status, AiAccountTarget, AiBillingMonth,
    AiBillingProject, AiBillingUsage, AiCreditKind, AiDecimal, AiMeteredCredit,
    AiMeteredModelStatus, AiMeteredOrgCharge, AiMeteredSummaryLine, AiOrgChargeKind, AiProvider,
    AiProviderAccountId, AiStoredProviderAccount, AiSummaryKind, AiTokenCounts, AiUnassignedKind,
    AiUsageDateRange, AiUsdNanos, UserId,
};
use crate::domain::AiMeteredUsageError;

/// A group is flagged when `G × 100 < 85 × T`. Not 90: a group used only
/// through Auto sits at exactly 0.9, since the payer's gross includes Auto's
/// discount and Toki cannot see Auto.
pub const LOW_RATIO: (i64, i64) = (85, 100);
/// The tracked cost "billed nothing" needs per account or group and month:
/// 100 credits ($1).
pub const FLAG_MATERIAL_NANOS: i64 = 1_000_000_000;
/// Residuals of at most this many nanodollars (0.00001 credits), rounding of
/// per-row `G`, count as zero.
pub const RESIDUAL_TOLERANCE_NANOS: i64 = 100;

/// Where a metered payer's tracked usage goes: the project its key resolves
/// to, one kind of Unassigned, or personal/internal (a key its user marked,
/// included and shown apart rather than dropped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiMeteredDestination {
    Project(AiBillingProject),
    Personal,
}

impl AiMeteredDestination {
    /// Bill order: mapped projects by name, then id; Unassigned unmapped,
    /// then unattributed; personal last.
    pub fn order_key(&self) -> (u8, String, String) {
        match self {
            Self::Project(AiBillingProject::Mapped(project)) => {
                (0, project.name.clone(), project.id.to_string())
            }
            Self::Project(AiBillingProject::Unassigned(AiUnassignedKind::Unmapped)) => {
                (1, String::new(), String::new())
            }
            Self::Project(AiBillingProject::Unassigned(AiUnassignedKind::Unattributed)) => {
                (2, String::new(), String::new())
            }
            Self::Personal => (3, String::new(), String::new()),
        }
    }

    /// What tells destinations apart: a project by its id, whatever its name
    /// (renamed between two keys' mappings), as the bill merges projects.
    pub(super) fn identity(&self) -> (u8, String) {
        let (rank, _, id) = self.order_key();
        (rank, id)
    }
}

/// Toki's tracked usage paid for by the payer, per user, UTC day, model and
/// destination. The destinations of one user, day and model sum exactly to
/// that model's rounded cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTrackedModelDay {
    pub user_id: UserId,
    pub day: Date,
    pub model: String,
    pub destination: AiMeteredDestination,
    pub tokens: AiTokenCounts,
    /// `T`'s known part: the priced cost.
    pub priced_cost: AiUsdNanos,
    pub records: i64,
    pub unpriced_records: i64,
    /// The priced cost of buckets on an old pricing basis.
    pub old_basis_cost: AiUsdNanos,
    pub old_basis_buckets: i64,
    /// The priced cost of buckets before the reprice floor, never repriced.
    pub before_floor_cost: AiUsdNanos,
}

impl AiTrackedModelDay {
    /// The usage as billing lines show it.
    pub fn usage(&self) -> AiBillingUsage {
        AiBillingUsage {
            tokens: self.tokens,
            records: self.records,
            priced_cost: self.priced_cost,
            unpriced_records: self.unpriced_records,
        }
    }
}

/// A stored credits row of an account on a UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredAccountCredit {
    pub account_id: AiProviderAccountId,
    pub day: Date,
    /// The login the row was requested for.
    pub login: String,
    pub credit: AiMeteredCredit,
    /// `G`.
    pub gross_usd: AiUsdNanos,
    pub fetched_at: OffsetDateTime,
}

/// A stored organization credits row on a UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredOrgCredit {
    pub day: Date,
    pub credit: AiMeteredCredit,
    pub gross_usd: AiUsdNanos,
}

/// The latest comparison of the payer's month total with the stored days, for
/// an account or (without one) the organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiUsageCheck {
    pub account_id: Option<AiProviderAccountId>,
    pub checked_at: OffsetDateTime,
    /// The payer's month total of gross quantities; `None` when the check
    /// failed.
    pub payer_gross: Option<AiDecimal>,
    pub stored_gross: AiDecimal,
    /// Whether every key's totals match; `None` when the check failed.
    pub matches: Option<bool>,
    pub error: Option<String>,
}

/// One day's import coverage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiDayCoverage {
    pub org_fetched: bool,
    pub org_complete: bool,
    /// Expected account slices never fetched.
    pub accounts_missing: u32,
    /// Fetched account slices not yet fetched after the day ended.
    pub accounts_incomplete: u32,
}

/// What reconciliation is computed from.
#[derive(Debug, Clone)]
pub struct AiReconciliationInput {
    pub payer: AiProvider,
    pub month: AiBillingMonth,
    pub today: Date,
    pub settle_days: u32,
    pub accounts: Vec<AiStoredProviderAccount>,
    pub account_credits: Vec<AiStoredAccountCredit>,
    pub org_credits: Vec<AiStoredOrgCredit>,
    pub org_charges: Vec<AiMeteredOrgCharge>,
    pub summary: Option<(Vec<AiMeteredSummaryLine>, OffsetDateTime)>,
    pub tracked: Vec<AiTrackedModelDay>,
    pub checks: Vec<AiUsageCheck>,
    /// The reconciled days, each with its coverage: the month's imported days
    /// through today.
    pub coverage: BTreeMap<Date, AiDayCoverage>,
    /// Each fetched account slice of the reconciled days, by account and day:
    /// whether it is complete (fetched after its day ended).
    pub account_slices: HashMap<(AiProviderAccountId, Date), bool>,
    /// One user's view: only days the accounts are linked to them count, and
    /// no residual is computed.
    pub focus: Option<UserId>,
}

/// Why an account or group needs a look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiReconciliationFlag {
    /// Material tracked usage on settled day-groups the payer billed nothing
    /// for.
    TrackedNotBilled,
    /// A settled day-group billed below 85% of its tracked usage.
    LowRatio,
    /// Tracked usage on an old pricing basis.
    StalePricing,
    /// A credits row or tracked model the catalog does not know.
    UnknownModel,
    /// Credits on days without a disposition.
    Unlinked,
    /// Credits on External days.
    External,
    /// The stored days differ from the payer's month total.
    PayerTotalMismatch,
    /// The account's import failed, though it held a seat in the month.
    ImportFailed,
}

/// What a group's credits are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiGroupKind {
    Credits(AiCreditKind),
    /// Toki usage without credits of the model.
    TrackedOnly,
}

/// One (SKU, model) group of an account over the month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiReconciliationGroup {
    /// `None` for Toki usage without credits.
    pub sku: Option<String>,
    pub model: String,
    /// The payer's raw labels.
    pub labels: Vec<String>,
    pub kind: AiGroupKind,
    pub status: Option<AiMeteredModelStatus>,
    /// Every day's `G`.
    pub gross: AiUsdNanos,
    /// `G` of Auto rows.
    pub auto_gross: AiUsdNanos,
    /// `G` on linked days.
    pub linked_gross: AiUsdNanos,
    pub tracked: AiUsdNanos,
    pub attributed: AiUsdNanos,
    pub untracked: AiUsdNanos,
    pub flags: Vec<AiReconciliationFlag>,
}

/// One account over the month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountReconciliation {
    pub account_id: AiProviderAccountId,
    pub login: String,
    /// The account's dispositions within the reconciled days.
    pub owners: Vec<(AiAccountTarget, AiUsageDateRange)>,
    /// Days with credits and no disposition.
    pub unlinked_days: u32,
    /// Reconciled days without the account's slice: nothing compared yet.
    pub days_not_imported: u32,
    pub gross: AiUsdNanos,
    pub repository_billed: AiUsdNanos,
    /// `G` of untrackable credits, part of `gross`.
    pub untrackable: AiUsdNanos,
    /// `T` on linked days.
    pub tracked: AiUsdNanos,
    pub tracked_old_basis: AiUsdNanos,
    pub tracked_before_floor: AiUsdNanos,
    pub unpriced_records: i64,
    pub attributed: AiUsdNanos,
    pub untracked: AiUsdNanos,
    pub unlinked: AiUsdNanos,
    pub external: AiUsdNanos,
    /// Attributed over trackable `G` on linked days, as (numerator,
    /// denominator); `None` without trackable credits.
    pub tracked_share: Option<(i64, i64)>,
    pub flags: Vec<AiReconciliationFlag>,
    pub check: Option<AiUsageCheck>,
    pub groups: Vec<AiReconciliationGroup>,
}

/// A user's tracked usage on days no disposition of theirs covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiUnlinkedTracked {
    pub user_id: UserId,
    pub tracked: AiUsdNanos,
    pub days: u32,
}

/// The organization's rows of one product, SKU, unit, repository and kind
/// over the month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryUsage {
    pub repository: String,
    pub product: String,
    pub sku: String,
    pub unit: String,
    pub kind: AiOrgChargeKind,
    pub quantity: AiDecimal,
    /// Credit kinds only.
    pub credit_usd: Option<AiUsdNanos>,
    pub gross_amount: AiDecimal,
    pub discount_amount: AiDecimal,
    pub net_amount: AiDecimal,
    pub days: u32,
}

/// Code review credits per account against those per repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiCodeReviewCheck {
    pub by_accounts: AiUsdNanos,
    pub by_repositories: AiUsdNanos,
}

/// How far a residual day can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiResidualStatus {
    Settled,
    /// Some slice is not complete yet, the day is within the settle days, or
    /// the organization's stored month does not equal the payer's month total
    /// (or was never checked).
    Provisional,
    /// A slice is missing, or a seated account's or the organization's check
    /// failed: an import gap, not unknown account.
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResidualGroup {
    pub sku: String,
    pub model: String,
    pub residual: i64,
}

/// One day's organization total against the per-account sum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResidualDay {
    pub day: Date,
    pub org_gross: AiUsdNanos,
    pub accounts_gross: AiUsdNanos,
    /// `org_gross − accounts_gross`, zero within the tolerance.
    pub residual: i64,
    pub status: AiResidualStatus,
    /// The accounts sum to more than the organization.
    pub accounts_exceed_org: bool,
    /// Per (SKU, model), only those beyond the tolerance.
    pub groups: Vec<AiResidualGroup>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiReconciliationTotals {
    pub gross: AiUsdNanos,
    pub repository_billed: AiUsdNanos,
    pub tracked: AiUsdNanos,
    pub tracked_old_basis: AiUsdNanos,
    pub tracked_before_floor: AiUsdNanos,
    pub attributed: AiUsdNanos,
    pub untracked: AiUsdNanos,
    pub unlinked: AiUsdNanos,
    pub external: AiUsdNanos,
    /// The residual over settled and provisional days.
    pub unknown_account: i64,
    pub unknown_account_settled: i64,
    /// The residual over incomplete days.
    pub incomplete_import: i64,
}

/// Credits with the pool discount, as quantities and amounts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiPoolCredits {
    pub gross_quantity: AiDecimal,
    pub discount_quantity: AiDecimal,
    pub net_quantity: AiDecimal,
    pub gross_amount: AiDecimal,
    pub discount_amount: AiDecimal,
    pub net_amount: AiDecimal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiPoolSeats {
    pub user_months: AiDecimal,
    pub gross_amount: AiDecimal,
    pub net_amount: AiDecimal,
}

/// The month's summary: credits and the pool discount, seats, and the
/// payer's bill so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPoolSummary {
    pub fetched_at: OffsetDateTime,
    /// The first UTC day with net credits: the pool ran out.
    pub exhausted_on: Option<Date>,
    pub credits: AiPoolCredits,
    pub seats: Option<AiPoolSeats>,
    /// Every row's net amount.
    pub net_total: AiDecimal,
    /// (product, SKU, unit) of rows not understood.
    pub unrecognized: Vec<(String, String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredReconciliation {
    pub payer: AiProvider,
    pub month: AiBillingMonth,
    /// Flagged first, then by gross, largest first, then by login.
    pub accounts: Vec<AiAccountReconciliation>,
    pub unlinked_tracked: Vec<AiUnlinkedTracked>,
    pub repositories: Vec<AiRepositoryUsage>,
    pub code_review: Option<AiCodeReviewCheck>,
    pub residual_days: Vec<AiResidualDay>,
    /// The latest check of the organization's stored month against the
    /// payer's month total, which the residual rests on. `None` in one user's
    /// view and before the first check.
    pub org_check: Option<AiUsageCheck>,
    pub totals: AiReconciliationTotals,
    pub pool: Option<AiPoolSummary>,
}

/// Whether the account held a seat on a day of `month` through `today`.
pub fn held_seat_in(account: &AiStoredProviderAccount, month: AiBillingMonth, today: Date) -> bool {
    let end = month.dates().end().min(today.next_day().unwrap_or(today));
    account
        .seated_days(today)
        .iter()
        .any(|range| range.start() < end && month.first_day() < range.end())
}

/// The first UTC day whose organization credits have a net quantity above
/// 0.000001: the day the pool ran out.
pub fn pool_exhausted_on(org_credits: &[AiStoredOrgCredit]) -> Option<Date> {
    let mut by_day: BTreeMap<Date, AiDecimal> = BTreeMap::new();
    for row in org_credits {
        let total = by_day.entry(row.day).or_default();
        *total = total
            .checked_add(&row.credit.amounts.net_quantity)
            .unwrap_or(*total);
    }
    by_day
        .into_iter()
        .find(|(_, net)| !net.matches(&AiDecimal::ZERO))
        .map(|(day, _)| day)
}

/// A month group being summed, in nanodollars.
#[derive(Default)]
struct GroupSum {
    labels: BTreeSet<String>,
    gross: i128,
    auto_gross: i128,
    linked_gross: i128,
    tracked: i128,
    attributed: i128,
    untracked: i128,
    zero_gross_tracked: i128,
    /// A settled day-group was billed below `LOW_RATIO` of its tracked usage.
    low_ratio: bool,
    stale: bool,
    unrecognized: bool,
}

/// An account being summed, in nanodollars.
#[derive(Default)]
struct AccountSum {
    gross: i128,
    repository_billed: i128,
    untrackable: i128,
    tracked: i128,
    tracked_old_basis: i128,
    tracked_before_floor: i128,
    unpriced_records: i64,
    attributed: i128,
    untracked: i128,
    unlinked: i128,
    external: i128,
    share_denominator: i128,
    zero_gross_tracked: i128,
    low_ratio: bool,
    stale: bool,
    unknown_model: bool,
    unlinked_days: u32,
    days_not_imported: u32,
    groups: BTreeMap<GroupKey, GroupSum>,
}

/// What a day-group is keyed by: (SKU, model, kind); the SKU is `None` for
/// Toki usage without credits.
pub(super) type GroupKey = (Option<String>, String, AiGroupKind);

/// A user's tracked usage of one model on one day, or of one destination of
/// it, in nanodollars.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct TrackedSum {
    pub priced: i128,
    pub records: i64,
    pub unpriced_records: i64,
    pub old_basis: i128,
    pub old_basis_buckets: i64,
    pub before_floor: i128,
    pub tokens: AiTokenCounts,
}

impl TrackedSum {
    fn add(&mut self, row: &AiTrackedModelDay) {
        self.priced += i128::from(row.priced_cost.nanos());
        self.records = self.records.saturating_add(row.records);
        self.unpriced_records = self.unpriced_records.saturating_add(row.unpriced_records);
        self.old_basis += i128::from(row.old_basis_cost.nanos());
        self.old_basis_buckets = self.old_basis_buckets.saturating_add(row.old_basis_buckets);
        self.before_floor += i128::from(row.before_floor_cost.nanos());
        let tokens = &mut self.tokens;
        tokens.input = tokens.input.saturating_add(row.tokens.input);
        tokens.cache_read = tokens.cache_read.saturating_add(row.tokens.cache_read);
        tokens.cache_write = tokens.cache_write.saturating_add(row.tokens.cache_write);
        tokens.output = tokens.output.saturating_add(row.tokens.output);
    }

    /// The usage as billing lines show it.
    pub fn usage(&self) -> Result<AiBillingUsage, AiMeteredUsageError> {
        let usage = AiBillingUsage {
            tokens: self.tokens,
            records: self.records,
            priced_cost: nanos(self.priced)?,
            unpriced_records: self.unpriced_records,
        };
        usage.ensure_reportable().map_err(|_| out_of_range())?;
        Ok(usage)
    }
}

/// A user's tracked usage of one model on one UTC day, in total and per
/// destination.
#[derive(Debug, Default, Clone)]
pub(super) struct ModelTracked {
    pub total: TrackedSum,
    /// In `AiMeteredDestination::order_key` order; they sum to `total`.
    pub destinations: Vec<(AiMeteredDestination, TrackedSum)>,
}

impl ModelTracked {
    fn add(&mut self, row: &AiTrackedModelDay) {
        self.total.add(row);
        let identity = row.destination.identity();
        match self
            .destinations
            .iter_mut()
            .find(|(destination, _)| destination.identity() == identity)
        {
            Some((_, sum)) => sum.add(row),
            None => {
                let mut sum = TrackedSum::default();
                sum.add(row);
                self.destinations.push((row.destination.clone(), sum));
            }
        }
    }
}

/// One (SKU, model, kind) group of an account's UTC day, classified as the
/// T/G rule sees it.
#[derive(Debug, Clone)]
pub(super) struct DayGroup<'a> {
    pub key: GroupKey,
    /// The day's credits rows of the group; none for Toki usage only.
    pub rows: Vec<&'a AiStoredAccountCredit>,
    /// `G`, in nanodollars.
    pub gross: i128,
    /// The weighed tracked model matched to the group on a linked day: free
    /// models, untrackable, unrecognized and repository-billed credits and
    /// unlinked and External days have none.
    pub total: Option<TrackedSum>,
    /// `total` per destination: the weights `T_d`.
    pub tracked: Vec<(AiMeteredDestination, TrackedSum)>,
    /// `split_group(G, T_d…)` on linked days: one charge per `tracked` entry.
    /// Empty on other days and for repository-billed credits.
    pub charges: Vec<i128>,
    /// What `split_group` left untracked; zero on other days and for
    /// repository-billed credits.
    pub untracked: i128,
}

impl DayGroup<'_> {
    fn new(key: GroupKey) -> Self {
        Self {
            key,
            rows: Vec::new(),
            gross: 0,
            total: None,
            tracked: Vec::new(),
            charges: Vec::new(),
            untracked: 0,
        }
    }

    pub fn kind(&self) -> AiGroupKind {
        self.key.2
    }

    /// `T`: the weighed tracked cost.
    pub fn tracked_cost(&self) -> i128 {
        self.total.map_or(0, |total| total.priced)
    }

    pub fn sku(&self) -> Option<&String> {
        self.key.0.as_ref()
    }

    pub fn model(&self) -> &str {
        &self.key.1
    }
}

/// One reconciled day of an account.
#[derive(Debug, Clone)]
pub(super) struct AccountDay<'a> {
    pub day: Date,
    pub owner: Option<AiAccountTarget>,
    /// The account's slice: `None` when it was never fetched, else whether it
    /// is complete.
    pub slice: Option<bool>,
    /// The slice is complete and the day is before the settle days: the payer
    /// is done posting it.
    pub settled: bool,
    /// Empty without a slice.
    pub groups: Vec<DayGroup<'a>>,
}

/// One account over the reconciled days.
#[derive(Debug, Clone)]
pub(super) struct AccountWalk<'a> {
    pub account: &'a AiStoredProviderAccount,
    pub check: Option<&'a AiUsageCheck>,
    /// It held a seat in the month.
    pub seated: bool,
    /// Its import failed without a seat in the month: its slices are not
    /// expected, so missing ones are no gap.
    pub excused: bool,
    /// The reconciled days, only the focused user's in one user's view.
    pub days: Vec<AccountDay<'a>>,
}

/// A month walked once per account and day: what reconciliation sums and
/// what the bill allocates, so the two agree by construction.
#[derive(Debug)]
pub(super) struct AiMeteredWalk<'a> {
    /// The month's imported days through today.
    pub days: Vec<Date>,
    pub window: Option<AiUsageDateRange>,
    /// The first day within the settle days.
    pub settling_from: Date,
    /// Tracked usage per user and day on the reconciled days, by model.
    pub tracked: HashMap<(UserId, Date), BTreeMap<String, ModelTracked>>,
    /// The user days some account is linked to.
    pub linked_user_days: HashSet<(UserId, Date)>,
    pub accounts: Vec<AccountWalk<'a>>,
    /// A seated account's import failed in the month.
    pub seated_errors: bool,
    pub org_check: Option<&'a AiUsageCheck>,
}

/// The T/G rule for one day-group, in nanodollars: one charge per tracked
/// destination (`tracked` holds each one's `T_d`) and the untracked rest.
///
/// - `G = 0`: nothing, every charge zero.
/// - `T ≥ G`: `G` split by `T_d` (`allocate_largest_remainder`, ties to the
///   earlier destination); nothing untracked.
/// - `T < G`: each destination gets its `T_d`; `G − T` is untracked.
///
/// The charges sum to `min(G, T)`, and none exceeds its `T_d`.
pub fn split_group(
    gross: i128,
    tracked: &[i128],
) -> Result<(Vec<i128>, i128), AiMeteredUsageError> {
    if gross < 0 || tracked.iter().any(|weight| *weight < 0) {
        return Err(out_of_range());
    }
    if gross == 0 {
        return Ok((vec![0; tracked.len()], 0));
    }
    let total: i128 = tracked.iter().sum();
    if total < gross {
        return Ok((tracked.to_vec(), gross - total));
    }
    let weights = tracked
        .iter()
        .map(|weight| u128::try_from(*weight).map_err(|_| out_of_range()))
        .collect::<Result<Vec<_>, _>>()?;
    let gross_units = i64::try_from(gross).map_err(|_| out_of_range())?;
    let charges = allocate_largest_remainder(gross_units, &weights)
        .map_err(|_| out_of_range())?
        .ok_or_else(out_of_range)?;
    Ok((charges.into_iter().map(i128::from).collect(), 0))
}

/// Walks a month: per account and reconciled day, the owner, the slice and
/// the day-groups with `split_group` applied. See the module documentation.
pub(super) fn walk(
    input: &AiReconciliationInput,
) -> Result<AiMeteredWalk<'_>, AiMeteredUsageError> {
    let settling_from = input.today - Duration::days(i64::from(input.settle_days));
    let days: Vec<Date> = input
        .coverage
        .keys()
        .copied()
        .filter(|day| input.month.contains(*day) && *day <= input.today)
        .collect();
    let day_set: HashSet<Date> = days.iter().copied().collect();
    let window = match (days.first(), days.last()) {
        (Some(first), Some(last)) => AiUsageDateRange::from_inclusive(*first, *last),
        _ => None,
    };

    let mut tracked: HashMap<(UserId, Date), BTreeMap<String, ModelTracked>> = HashMap::new();
    for row in input
        .tracked
        .iter()
        .filter(|row| day_set.contains(&row.day))
        .filter(|row| input.focus.is_none_or(|user| row.user_id == user))
    {
        tracked
            .entry((row.user_id, row.day))
            .or_default()
            .entry(row.model.clone())
            .or_default()
            .add(row);
    }
    for models in tracked.values_mut() {
        for model in models.values_mut() {
            model
                .destinations
                .sort_by_cached_key(|(destination, _)| destination.order_key());
        }
    }

    let mut credits_by_account: HashMap<(AiProviderAccountId, Date), Vec<&AiStoredAccountCredit>> =
        HashMap::new();
    for row in input
        .account_credits
        .iter()
        .filter(|row| day_set.contains(&row.day))
    {
        credits_by_account
            .entry((row.account_id, row.day))
            .or_default()
            .push(row);
    }
    let checks_by_account: HashMap<AiProviderAccountId, &AiUsageCheck> = input
        .checks
        .iter()
        .filter_map(|check| check.account_id.map(|account| (account, check)))
        .collect();

    let mut linked_user_days: HashSet<(UserId, Date)> = HashSet::new();
    let mut seated_errors = false;
    let mut accounts = Vec::with_capacity(input.accounts.len());
    for account in &input.accounts {
        let check = checks_by_account.get(&account.account_id).copied();
        let seated = held_seat_in(account, input.month, input.today);
        let failed = check.is_some_and(|check| check.error.is_some());
        if failed && seated {
            seated_errors = true;
        }
        let mut account_days = Vec::new();
        for day in &days {
            let owner = account
                .disposition_on(*day)
                .map(|disposition| disposition.terms.target);
            if let Some(user) = input.focus {
                if owner != Some(AiAccountTarget::User(user)) {
                    continue;
                }
            }
            if let Some(AiAccountTarget::User(user)) = owner {
                linked_user_days.insert((user, *day));
            }
            let slice = input
                .account_slices
                .get(&(account.account_id, *day))
                .copied();
            let groups = match slice {
                None => Vec::new(),
                Some(_) => {
                    let rows = credits_by_account
                        .get(&(account.account_id, *day))
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    day_groups(input.payer, owner, *day, rows, &tracked)?
                }
            };
            account_days.push(AccountDay {
                day: *day,
                owner,
                slice,
                settled: slice == Some(true) && *day < settling_from,
                groups,
            });
        }
        accounts.push(AccountWalk {
            account,
            check,
            seated,
            excused: failed && !seated,
            days: account_days,
        });
    }

    Ok(AiMeteredWalk {
        days,
        window,
        settling_from,
        tracked,
        linked_user_days,
        accounts,
        seated_errors,
        org_check: input.checks.iter().find(|check| check.account_id.is_none()),
    })
}

/// An imported account day's groups. Credits form groups of (SKU, model,
/// kind); on a linked day each weighed tracked model joins the trackable group
/// of its model, or forms a group of Toki usage only (`G = 0`), and free
/// models are listed without weight.
fn day_groups<'a>(
    payer: AiProvider,
    owner: Option<AiAccountTarget>,
    day: Date,
    rows: &[&'a AiStoredAccountCredit],
    tracked: &HashMap<(UserId, Date), BTreeMap<String, ModelTracked>>,
) -> Result<Vec<DayGroup<'a>>, AiMeteredUsageError> {
    let mut groups: BTreeMap<GroupKey, DayGroup<'a>> = BTreeMap::new();
    for row in rows {
        let credit = &row.credit;
        let key: GroupKey = (
            Some(credit.sku.clone()),
            credit.model.model.clone(),
            AiGroupKind::Credits(credit.kind),
        );
        let group = groups
            .entry(key.clone())
            .or_insert_with(|| DayGroup::new(key));
        group.rows.push(row);
        group.gross += i128::from(row.gross_usd.nanos());
    }

    let Some(AiAccountTarget::User(user)) = owner else {
        return Ok(groups.into_values().collect());
    };
    if let Some(models) = tracked.get(&(user, day)) {
        for (model, sums) in models {
            let trackable = groups
                .keys()
                .find(|(_, group_model, kind)| {
                    group_model == model && *kind == AiGroupKind::Credits(AiCreditKind::Trackable)
                })
                .cloned();
            let key = trackable.unwrap_or_else(|| (None, model.clone(), AiGroupKind::TrackedOnly));
            let group = groups
                .entry(key.clone())
                .or_insert_with(|| DayGroup::new(key));
            if metered_model_status(payer, model) == Some(AiMeteredModelStatus::Free) {
                // Free models carry no weight: listed, with `T = 0`.
                continue;
            }
            group.total = Some(sums.total);
            group.tracked = sums.destinations.clone();
        }
    }
    for group in groups.values_mut() {
        if group.kind() == AiGroupKind::Credits(AiCreditKind::RepositoryBilled) {
            continue;
        }
        let weights: Vec<i128> = group.tracked.iter().map(|(_, sum)| sum.priced).collect();
        let (charges, untracked) = split_group(group.gross, &weights)?;
        group.charges = charges;
        group.untracked = untracked;
    }
    Ok(groups.into_values().collect())
}

fn nanos(value: i128) -> Result<AiUsdNanos, AiMeteredUsageError> {
    i64::try_from(value)
        .ok()
        .and_then(AiUsdNanos::new)
        .ok_or_else(out_of_range)
}

fn signed(value: i128) -> Result<i64, AiMeteredUsageError> {
    i64::try_from(value).map_err(|_| out_of_range())
}

pub(super) fn out_of_range() -> AiMeteredUsageError {
    AiMeteredUsageError::Storage("reconciliation totals exceed the supported range".to_string())
}

fn sum_decimal(total: &mut AiDecimal, value: &AiDecimal) -> Result<(), AiMeteredUsageError> {
    *total = total.checked_add(value).ok_or_else(out_of_range)?;
    Ok(())
}

/// Reconciles a month; see the module documentation.
pub fn reconcile(
    input: AiReconciliationInput,
) -> Result<AiMeteredReconciliation, AiMeteredUsageError> {
    let walk = walk(&input)?;
    let (payer, month, focus) = (input.payer, input.month, input.focus);
    let window = walk.window;
    let org_check = walk.org_check.cloned();

    let mut reconciled: Vec<(AiAccountReconciliation, bool)> = Vec::new();
    let mut totals = AccountSum::default();

    for account_walk in &walk.accounts {
        let account = account_walk.account;
        let mut sum = AccountSum::default();
        for day in &account_walk.days {
            if day.slice.is_none() {
                sum.days_not_imported += 1;
                continue;
            }
            reconcile_day(payer, &mut sum, day);
        }

        let check = account_walk.check.cloned();
        let seated = account_walk.seated;
        let mut flags = Vec::new();
        let material = i128::from(FLAG_MATERIAL_NANOS);
        if sum.zero_gross_tracked >= material {
            flags.push(AiReconciliationFlag::TrackedNotBilled);
        }
        if sum.low_ratio {
            flags.push(AiReconciliationFlag::LowRatio);
        }
        if sum.stale {
            flags.push(AiReconciliationFlag::StalePricing);
        }
        if sum.unknown_model {
            flags.push(AiReconciliationFlag::UnknownModel);
        }
        if sum.unlinked > 0 {
            flags.push(AiReconciliationFlag::Unlinked);
        }
        if sum.external > 0 {
            flags.push(AiReconciliationFlag::External);
        }
        if let Some(check) = &check {
            if check.matches == Some(false) {
                flags.push(AiReconciliationFlag::PayerTotalMismatch);
            }
            if check.error.is_some() && seated {
                flags.push(AiReconciliationFlag::ImportFailed);
            }
        }

        let has_linked_days = match focus {
            Some(user) => account.dispositions.iter().any(|disposition| {
                disposition.terms.target == AiAccountTarget::User(user)
                    && window.is_some_and(|window| {
                        disposition.terms.period.to_date_range(window).is_some()
                    })
            }),
            None => false,
        };
        let listed = sum.gross > 0
            || sum.tracked > 0
            || has_linked_days
            || (focus.is_none()
                && (seated
                    || check.as_ref().is_some_and(|check| {
                        check.error.is_some() || check.matches == Some(false)
                    })));

        let owners = window
            .map(|window| {
                account
                    .dispositions
                    .iter()
                    .filter(|disposition| {
                        focus.is_none_or(|user| {
                            disposition.terms.target == AiAccountTarget::User(user)
                        })
                    })
                    .filter_map(|disposition| {
                        disposition
                            .terms
                            .period
                            .to_date_range(window)
                            .map(|range| (disposition.terms.target, range))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let groups = sum
            .groups
            .iter()
            .map(|((sku, model, kind), group)| month_group(payer, sku, model, *kind, group))
            .collect::<Result<Vec<_>, _>>()?;
        let tracked_share = (sum.share_denominator > 0)
            .then(|| {
                Ok::<_, AiMeteredUsageError>((
                    signed(sum.attributed)?,
                    signed(sum.share_denominator)?,
                ))
            })
            .transpose()?;

        let row = AiAccountReconciliation {
            account_id: account.account_id,
            login: account.login.as_str().to_string(),
            owners,
            unlinked_days: sum.unlinked_days,
            days_not_imported: sum.days_not_imported,
            gross: nanos(sum.gross)?,
            repository_billed: nanos(sum.repository_billed)?,
            untrackable: nanos(sum.untrackable)?,
            tracked: nanos(sum.tracked)?,
            tracked_old_basis: nanos(sum.tracked_old_basis)?,
            tracked_before_floor: nanos(sum.tracked_before_floor)?,
            unpriced_records: sum.unpriced_records,
            attributed: nanos(sum.attributed)?,
            untracked: nanos(sum.untracked)?,
            unlinked: nanos(sum.unlinked)?,
            external: nanos(sum.external)?,
            tracked_share,
            flags,
            check,
            groups,
        };
        totals.gross += sum.gross;
        totals.repository_billed += sum.repository_billed;
        totals.tracked += sum.tracked;
        totals.tracked_old_basis += sum.tracked_old_basis;
        totals.tracked_before_floor += sum.tracked_before_floor;
        totals.attributed += sum.attributed;
        totals.untracked += sum.untracked;
        totals.unlinked += sum.unlinked;
        totals.external += sum.external;
        reconciled.push((row, listed));
    }

    let mut account_rows: Vec<AiAccountReconciliation> = reconciled
        .into_iter()
        .filter_map(|(row, listed)| listed.then_some(row))
        .collect();
    account_rows.sort_by_cached_key(|row| {
        (
            row.flags.is_empty(),
            std::cmp::Reverse(row.gross),
            row.login.to_lowercase(),
            row.account_id,
        )
    });

    // Tracked usage on days no disposition of the user covers; free models
    // carry no weight.
    let mut unlinked_by_user: BTreeMap<i32, (i128, u32)> = BTreeMap::new();
    for ((user, day), models) in &walk.tracked {
        if walk.linked_user_days.contains(&(*user, *day)) {
            continue;
        }
        let mut weighed = models
            .iter()
            .filter(|(model, _)| {
                metered_model_status(payer, model) != Some(AiMeteredModelStatus::Free)
            })
            .peekable();
        if weighed.peek().is_none() {
            continue;
        }
        let entry = unlinked_by_user.entry(user.as_i32()).or_default();
        entry.0 += weighed.map(|(_, model)| model.total.priced).sum::<i128>();
        entry.1 += 1;
    }
    let mut unlinked_tracked = unlinked_by_user
        .into_iter()
        .map(|(user, (tracked, days))| {
            Ok(AiUnlinkedTracked {
                user_id: UserId::new(user),
                tracked: nanos(tracked)?,
                days,
            })
        })
        .collect::<Result<Vec<_>, AiMeteredUsageError>>()?;
    unlinked_tracked.sort_by_key(|row| (std::cmp::Reverse(row.tracked), row.user_id.as_i32()));

    let mut result_totals = AiReconciliationTotals {
        gross: nanos(totals.gross)?,
        repository_billed: nanos(totals.repository_billed)?,
        tracked: nanos(totals.tracked)?,
        tracked_old_basis: nanos(totals.tracked_old_basis)?,
        tracked_before_floor: nanos(totals.tracked_before_floor)?,
        attributed: nanos(totals.attributed)?,
        untracked: nanos(totals.untracked)?,
        unlinked: nanos(totals.unlinked)?,
        external: nanos(totals.external)?,
        ..AiReconciliationTotals::default()
    };

    if focus.is_some() {
        return Ok(AiMeteredReconciliation {
            payer,
            month,
            accounts: account_rows,
            unlinked_tracked,
            repositories: Vec::new(),
            code_review: None,
            residual_days: Vec::new(),
            org_check: None,
            totals: result_totals,
            pool: None,
        });
    }

    let residual_days = residual_days(
        &walk.days,
        &input.coverage,
        &input.org_credits,
        &input.account_credits,
        ResidualTrust {
            settling_from: walk.settling_from,
            seated_errors: walk.seated_errors,
            org_check: walk.org_check,
        },
    )?;
    let (mut unknown, mut unknown_settled, mut incomplete) = (0_i128, 0_i128, 0_i128);
    for day in &residual_days {
        let residual = i128::from(day.residual);
        match day.status {
            AiResidualStatus::Settled => {
                unknown += residual;
                unknown_settled += residual;
            }
            AiResidualStatus::Provisional => unknown += residual,
            AiResidualStatus::Incomplete => incomplete += residual,
        }
    }
    result_totals.unknown_account = signed(unknown)?;
    result_totals.unknown_account_settled = signed(unknown_settled)?;
    result_totals.incomplete_import = signed(incomplete)?;

    let charges: Vec<&AiMeteredOrgCharge> = input
        .org_charges
        .iter()
        .filter(|charge| month.contains(charge.day))
        .collect();
    let repositories = repository_usage(&charges)?;
    let day_set: HashSet<Date> = walk.days.iter().copied().collect();
    let by_accounts: i128 = input
        .account_credits
        .iter()
        .filter(|row| day_set.contains(&row.day))
        .filter(|row| row.credit.kind == AiCreditKind::RepositoryBilled)
        .map(|row| i128::from(row.gross_usd.nanos()))
        .sum();
    let by_repositories: i128 = charges
        .iter()
        .filter(|charge| charge.kind == AiOrgChargeKind::RepositoryCredits)
        .filter_map(|charge| charge.credit_usd())
        .map(|usd| i128::from(usd.nanos()))
        .sum();
    let code_review = (by_accounts > 0 || by_repositories > 0)
        .then(|| {
            Ok::<_, AiMeteredUsageError>(AiCodeReviewCheck {
                by_accounts: nanos(by_accounts)?,
                by_repositories: nanos(by_repositories)?,
            })
        })
        .transpose()?;
    let month_org_credits: Vec<AiStoredOrgCredit> = input
        .org_credits
        .iter()
        .filter(|row| month.contains(row.day))
        .cloned()
        .collect();
    let pool = input
        .summary
        .as_ref()
        .map(|(lines, fetched_at)| pool_summary(lines, *fetched_at, &month_org_credits))
        .transpose()?;

    Ok(AiMeteredReconciliation {
        payer,
        month,
        accounts: account_rows,
        unlinked_tracked,
        repositories,
        code_review,
        residual_days,
        org_check,
        totals: result_totals,
        pool,
    })
}

/// Adds one imported account day to its sums.
fn reconcile_day(payer: AiProvider, sum: &mut AccountSum, day: &AccountDay<'_>) {
    let mut owned_gross = 0_i128;
    for group in &day.groups {
        let month_group = sum.groups.entry(group.key.clone()).or_default();
        for row in &group.rows {
            let gross = i128::from(row.gross_usd.nanos());
            let credit = &row.credit;
            month_group.labels.insert(credit.model.label.clone());
            month_group.gross += gross;
            if credit.model.auto_routed {
                month_group.auto_gross += gross;
            }
            sum.gross += gross;
            match credit.kind {
                AiCreditKind::RepositoryBilled => {
                    sum.repository_billed += gross;
                    continue;
                }
                AiCreditKind::Unrecognized => {
                    month_group.unrecognized = true;
                    sum.unknown_model = true;
                }
                AiCreditKind::Untrackable => sum.untrackable += gross,
                AiCreditKind::Trackable => {}
            }
            if matches!(
                credit.kind,
                AiCreditKind::Trackable | AiCreditKind::Untrackable
            ) && metered_model_status(payer, &credit.model.model).is_none()
            {
                sum.unknown_model = true;
            }
            owned_gross += gross;
        }
    }

    match day.owner {
        None => {
            sum.unlinked += owned_gross;
            if owned_gross > 0 {
                sum.unlinked_days += 1;
            }
            return;
        }
        Some(AiAccountTarget::External) => {
            sum.external += owned_gross;
            return;
        }
        Some(AiAccountTarget::User(_)) => {}
    }

    for group in &day.groups {
        if group.kind() == AiGroupKind::Credits(AiCreditKind::RepositoryBilled) {
            continue;
        }
        let status = metered_model_status(payer, group.model());
        let month_group = sum.groups.entry(group.key.clone()).or_default();
        if let Some(total) = &group.total {
            sum.tracked += total.priced;
            sum.tracked_old_basis += total.old_basis;
            sum.tracked_before_floor += total.before_floor;
            sum.unpriced_records += total.unpriced_records;
            if total.old_basis_buckets > 0 {
                sum.stale = true;
                month_group.stale = true;
            }
            if total.priced > 0 && status.is_none() {
                sum.unknown_model = true;
            }
        }
        let gross = group.gross;
        let tracked = group.tracked_cost();
        let attributed: i128 = group.charges.iter().sum();
        let untracked = group.untracked;
        sum.attributed += attributed;
        sum.untracked += untracked;
        if group.kind() == AiGroupKind::Credits(AiCreditKind::Trackable) {
            sum.share_denominator += gross;
        }
        let zero_gross = day.settled && gross == 0 && tracked > 0;
        let low_ratio = day.settled && is_low_ratio(gross, tracked);
        month_group.linked_gross += gross;
        month_group.tracked += tracked;
        month_group.attributed += attributed;
        month_group.untracked += untracked;
        if zero_gross {
            month_group.zero_gross_tracked += tracked;
            sum.zero_gross_tracked += tracked;
        }
        if low_ratio {
            month_group.low_ratio = true;
            sum.low_ratio = true;
        }
    }
}

/// Whether a day-group with credits was billed below `LOW_RATIO` of its
/// tracked usage: `G × 100 < 85 × T`.
pub(super) fn is_low_ratio(gross: i128, tracked: i128) -> bool {
    gross > 0 && gross * i128::from(LOW_RATIO.1) < i128::from(LOW_RATIO.0) * tracked
}

fn month_group(
    payer: AiProvider,
    sku: &Option<String>,
    model: &str,
    kind: AiGroupKind,
    group: &GroupSum,
) -> Result<AiReconciliationGroup, AiMeteredUsageError> {
    let repository_billed = kind == AiGroupKind::Credits(AiCreditKind::RepositoryBilled);
    let status = (!repository_billed)
        .then(|| metered_model_status(payer, model))
        .flatten();
    let material = i128::from(FLAG_MATERIAL_NANOS);
    let mut flags = Vec::new();
    if group.zero_gross_tracked >= material {
        flags.push(AiReconciliationFlag::TrackedNotBilled);
    }
    if group.low_ratio {
        flags.push(AiReconciliationFlag::LowRatio);
    }
    if group.stale {
        flags.push(AiReconciliationFlag::StalePricing);
    }
    let unknown = group.unrecognized
        || (!repository_billed
            && status.is_none()
            && (kind != AiGroupKind::TrackedOnly || group.tracked > 0));
    if unknown {
        flags.push(AiReconciliationFlag::UnknownModel);
    }
    Ok(AiReconciliationGroup {
        sku: sku.clone(),
        model: model.to_string(),
        labels: group.labels.iter().cloned().collect(),
        kind,
        status,
        gross: nanos(group.gross)?,
        auto_gross: nanos(group.auto_gross)?,
        linked_gross: nanos(group.linked_gross)?,
        tracked: nanos(group.tracked)?,
        attributed: nanos(group.attributed)?,
        untracked: nanos(group.untracked)?,
        flags,
    })
}

/// What a residual day's status weighs besides its own coverage.
pub(super) struct ResidualTrust<'a> {
    /// The first day within the settle days.
    pub settling_from: Date,
    /// A seated account's import failed in the month.
    pub seated_errors: bool,
    pub org_check: Option<&'a AiUsageCheck>,
}

/// The organization's credits minus every account's, per day of `days`.
pub(super) fn residual_days(
    days: &[Date],
    coverage: &BTreeMap<Date, AiDayCoverage>,
    org_credits: &[AiStoredOrgCredit],
    account_credits: &[AiStoredAccountCredit],
    ResidualTrust {
        settling_from,
        seated_errors,
        org_check,
    }: ResidualTrust<'_>,
) -> Result<Vec<AiResidualDay>, AiMeteredUsageError> {
    type GroupKey = (String, String);
    let mut org: HashMap<Date, BTreeMap<GroupKey, i128>> = HashMap::new();
    for row in org_credits {
        *org.entry(row.day)
            .or_default()
            .entry((row.credit.sku.clone(), row.credit.model.model.clone()))
            .or_default() += i128::from(row.gross_usd.nanos());
    }
    let mut accounts: HashMap<Date, BTreeMap<GroupKey, i128>> = HashMap::new();
    for row in account_credits {
        *accounts
            .entry(row.day)
            .or_default()
            .entry((row.credit.sku.clone(), row.credit.model.model.clone()))
            .or_default() += i128::from(row.gross_usd.nanos());
    }
    let tolerance = i128::from(RESIDUAL_TOLERANCE_NANOS);
    // The organization's stored days are confirmed only by a matching check;
    // a failed one leaves them an import gap.
    let org_failed = org_check.is_some_and(|check| check.error.is_some());
    let org_confirmed = org_check.is_some_and(|check| check.matches == Some(true));
    let empty = BTreeMap::new();

    days.iter()
        .map(|day| {
            let org_groups = org.get(day).unwrap_or(&empty);
            let account_groups = accounts.get(day).unwrap_or(&empty);
            let org_gross: i128 = org_groups.values().sum();
            let accounts_gross: i128 = account_groups.values().sum();
            let raw = org_gross - accounts_gross;
            let residual = if raw.abs() <= tolerance { 0 } else { raw };
            let keys: BTreeSet<&GroupKey> =
                org_groups.keys().chain(account_groups.keys()).collect();
            let groups = keys
                .into_iter()
                .filter_map(|key| {
                    let difference = org_groups.get(key).copied().unwrap_or(0)
                        - account_groups.get(key).copied().unwrap_or(0);
                    (difference.abs() > tolerance).then(|| {
                        Ok(AiResidualGroup {
                            sku: key.0.clone(),
                            model: key.1.clone(),
                            residual: signed(difference)?,
                        })
                    })
                })
                .collect::<Result<Vec<_>, AiMeteredUsageError>>()?;
            let cover = coverage.get(day).copied().unwrap_or_default();
            let status = if !cover.org_fetched
                || cover.accounts_missing > 0
                || seated_errors
                || org_failed
            {
                AiResidualStatus::Incomplete
            } else if !cover.org_complete
                || cover.accounts_incomplete > 0
                || *day >= settling_from
                || !org_confirmed
            {
                AiResidualStatus::Provisional
            } else {
                AiResidualStatus::Settled
            };
            Ok(AiResidualDay {
                day: *day,
                org_gross: nanos(org_gross)?,
                accounts_gross: nanos(accounts_gross)?,
                residual: signed(residual)?,
                status,
                accounts_exceed_org: residual < -tolerance,
                groups,
            })
        })
        .collect()
}

fn repository_usage(
    charges: &[&AiMeteredOrgCharge],
) -> Result<Vec<AiRepositoryUsage>, AiMeteredUsageError> {
    type Key = (AiOrgChargeKind, String, String, String, String);
    let mut sums: BTreeMap<Key, (AiRepositoryUsage, i128, BTreeSet<Date>)> = BTreeMap::new();
    for charge in charges {
        let key: Key = (
            charge.kind,
            charge.repository.clone(),
            charge.product.clone(),
            charge.sku.clone(),
            charge.unit.clone(),
        );
        let (usage, credit, days) = sums.entry(key).or_insert_with(|| {
            (
                AiRepositoryUsage {
                    repository: charge.repository.clone(),
                    product: charge.product.clone(),
                    sku: charge.sku.clone(),
                    unit: charge.unit.clone(),
                    kind: charge.kind,
                    quantity: AiDecimal::ZERO,
                    credit_usd: None,
                    gross_amount: AiDecimal::ZERO,
                    discount_amount: AiDecimal::ZERO,
                    net_amount: AiDecimal::ZERO,
                    days: 0,
                },
                0,
                BTreeSet::new(),
            )
        });
        sum_decimal(&mut usage.quantity, &charge.quantity)?;
        sum_decimal(&mut usage.gross_amount, &charge.gross_amount)?;
        sum_decimal(&mut usage.discount_amount, &charge.discount_amount)?;
        sum_decimal(&mut usage.net_amount, &charge.net_amount)?;
        if let Some(usd) = charge.credit_usd() {
            *credit += i128::from(usd.nanos());
        }
        days.insert(charge.day);
    }
    let mut rows = sums
        .into_values()
        .map(|(mut usage, credit, days)| {
            if usage.kind.is_credits() {
                usage.credit_usd = Some(nanos(credit)?);
            }
            usage.days = u32::try_from(days.len()).unwrap_or(u32::MAX);
            Ok(usage)
        })
        .collect::<Result<Vec<_>, AiMeteredUsageError>>()?;
    rows.sort_by(|a, b| {
        (
            a.kind,
            std::cmp::Reverse(a.credit_usd),
            &a.repository,
            &a.sku,
        )
            .cmp(&(
                b.kind,
                std::cmp::Reverse(b.credit_usd),
                &b.repository,
                &b.sku,
            ))
    });
    Ok(rows)
}

fn pool_summary(
    lines: &[AiMeteredSummaryLine],
    fetched_at: OffsetDateTime,
    org_credits: &[AiStoredOrgCredit],
) -> Result<AiPoolSummary, AiMeteredUsageError> {
    let mut credits = AiPoolCredits::default();
    let mut seats: Option<AiPoolSeats> = None;
    let mut net_total = AiDecimal::ZERO;
    let mut unrecognized = Vec::new();
    for line in lines {
        let amounts = &line.amounts;
        sum_decimal(&mut net_total, &amounts.net_amount)?;
        match line.kind {
            AiSummaryKind::Credits | AiSummaryKind::UntrackableCredits => {
                sum_decimal(&mut credits.gross_quantity, &amounts.gross_quantity)?;
                sum_decimal(&mut credits.discount_quantity, &amounts.discount_quantity)?;
                sum_decimal(&mut credits.net_quantity, &amounts.net_quantity)?;
                sum_decimal(&mut credits.gross_amount, &amounts.gross_amount)?;
                sum_decimal(&mut credits.discount_amount, &amounts.discount_amount)?;
                sum_decimal(&mut credits.net_amount, &amounts.net_amount)?;
            }
            AiSummaryKind::Seats => {
                let seats = seats.get_or_insert_with(AiPoolSeats::default);
                sum_decimal(&mut seats.user_months, &amounts.gross_quantity)?;
                sum_decimal(&mut seats.gross_amount, &amounts.gross_amount)?;
                sum_decimal(&mut seats.net_amount, &amounts.net_amount)?;
            }
            AiSummaryKind::Unrecognized => {
                unrecognized.push((line.product.clone(), line.sku.clone(), line.unit.clone()))
            }
        }
    }
    Ok(AiPoolSummary {
        fetched_at,
        exhausted_on: pool_exhausted_on(org_credits),
        credits,
        seats,
        net_total,
        unrecognized,
    })
}

#[cfg(test)]
mod tests;
