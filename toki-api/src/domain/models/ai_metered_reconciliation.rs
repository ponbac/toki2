//! Reconciliation of what a metered payer billed (`G`, gross credits) with
//! what Toki tracked (`T`, Toki's estimate of the usage the payer paid for, at
//! the payer's price), per account, UTC day and group of (SKU, model). Nothing
//! here bills: it shows what the T/G rule would attribute.
//!
//! - **Owners.** Each account day belongs to the account's disposition that
//!   day: a user (linked), External, or nobody (unlinked).
//! - **Groups.** On linked days, credits form groups of (SKU, normalized
//!   model). A trackable group's `T` is the user's priced tracked cost of that
//!   model that day; untrackable and unrecognized groups have `T = 0`;
//!   repository-billed credits (code review) are left out, since the payer
//!   bills them per repository. A tracked model without credits that day forms
//!   a group with Toki usage only (`G = 0`).
//! - **The rule.** Per day and group, `attributed = min(G, T)` and
//!   `untracked = G − attributed`. Unpriced usage carries no weight, so a
//!   group with only unpriced usage has `T = 0`. Free models carry none
//!   either: their tracked cost is left out of `T` everywhere (old pricing
//!   basis and tracked without a linked account included), and their groups
//!   are listed with `T = 0`.
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
//!   missing slice are an import gap too, as is every day of a month with a
//!   failed seated account or an interrupted one, and days that may still
//!   change are provisional.
//!
//! Amounts are nanodollars (a credit is 10^7). Signed figures (residuals) are
//! plain `i64` nanodollars.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use time::{Date, Duration, OffsetDateTime};

use super::{
    metered_model_status, AiAccountTarget, AiBillingMonth, AiCreditKind, AiDecimal,
    AiMeteredCredit, AiMeteredModelStatus, AiMeteredOrgCharge, AiMeteredSummaryLine,
    AiOrgChargeKind, AiProvider, AiProviderAccountId, AiStoredProviderAccount, AiSummaryKind,
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

/// Toki's tracked usage paid for by the payer, per user, UTC day and model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTrackedModelDay {
    pub user_id: UserId,
    pub day: Date,
    pub model: String,
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
    /// Whether a run started importing the month and stopped, or died,
    /// before finishing this scope (a failed check, `error` saying why). Its
    /// stored days may be half replaced, or older than the organization's
    /// refreshed days, so it counts whatever the account's seat: never
    /// informational, and its month is imported again.
    pub interrupted: bool,
}

impl AiUsageCheck {
    /// The check of a scope whose month a run started importing at
    /// `started_at` and stopped before finishing, for `stop`, the run's
    /// error ("interrupted" when its process died).
    pub fn interrupted(
        account_id: Option<AiProviderAccountId>,
        started_at: OffsetDateTime,
        stored_gross: AiDecimal,
        stop: &str,
    ) -> Self {
        Self {
            account_id,
            checked_at: started_at,
            payer_gross: None,
            stored_gross,
            matches: None,
            error: Some(interrupted_check_error(stop)),
            interrupted: true,
        }
    }
}

/// The error of a check whose month a run stopped importing for `stop`.
pub fn interrupted_check_error(stop: &str) -> String {
    format!("the run stopped before finishing this month: {stop}")
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
    /// The account's import failed, though it held a seat in the month, or a
    /// run stopped before finishing its month (whatever its seat).
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
    /// A slice is missing, or a seated account's, an interrupted account's
    /// or the organization's check failed: an import gap, not unknown
    /// account.
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
    groups: BTreeMap<(Option<String>, String, AiGroupKind), GroupSum>,
}

/// A user's tracked usage of one model on one day.
#[derive(Default, Clone, Copy)]
struct TrackedSum {
    priced: i128,
    records: i64,
    unpriced_records: i64,
    old_basis: i128,
    old_basis_buckets: i64,
    before_floor: i128,
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

fn out_of_range() -> AiMeteredUsageError {
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
    let AiReconciliationInput {
        payer,
        month,
        today,
        settle_days,
        accounts,
        account_credits,
        org_credits,
        org_charges,
        summary,
        tracked,
        checks,
        coverage,
        account_slices,
        focus,
    } = input;
    let settling_from = today - Duration::days(i64::from(settle_days));
    let days: Vec<Date> = coverage
        .keys()
        .copied()
        .filter(|day| month.contains(*day) && *day <= today)
        .collect();
    let day_set: HashSet<Date> = days.iter().copied().collect();
    let window = match (days.first(), days.last()) {
        (Some(first), Some(last)) => AiUsageDateRange::from_inclusive(*first, *last),
        _ => None,
    };

    // The user's tracked usage per day and model.
    let mut tracked_by_day: HashMap<(UserId, Date), BTreeMap<String, TrackedSum>> = HashMap::new();
    for row in tracked
        .iter()
        .filter(|row| day_set.contains(&row.day))
        .filter(|row| focus.is_none_or(|user| row.user_id == user))
    {
        let sum = tracked_by_day
            .entry((row.user_id, row.day))
            .or_default()
            .entry(row.model.clone())
            .or_default();
        sum.priced += i128::from(row.priced_cost.nanos());
        sum.records += row.records;
        sum.unpriced_records += row.unpriced_records;
        sum.old_basis += i128::from(row.old_basis_cost.nanos());
        sum.old_basis_buckets += row.old_basis_buckets;
        sum.before_floor += i128::from(row.before_floor_cost.nanos());
    }

    let mut credits_by_account: HashMap<(AiProviderAccountId, Date), Vec<&AiStoredAccountCredit>> =
        HashMap::new();
    for row in account_credits
        .iter()
        .filter(|row| day_set.contains(&row.day))
    {
        credits_by_account
            .entry((row.account_id, row.day))
            .or_default()
            .push(row);
    }
    let checks_by_account: HashMap<AiProviderAccountId, &AiUsageCheck> = checks
        .iter()
        .filter_map(|check| check.account_id.map(|account| (account, check)))
        .collect();
    let org_check = checks
        .iter()
        .find(|check| check.account_id.is_none())
        .cloned();

    let mut linked_user_days: HashSet<(UserId, Date)> = HashSet::new();
    let mut reconciled: Vec<(AiAccountReconciliation, bool)> = Vec::new();
    let mut totals = AccountSum::default();
    let mut import_failed = false;

    for account in &accounts {
        let mut sum = AccountSum::default();
        for day in &days {
            let owner = account
                .disposition_on(*day)
                .map(|disposition| disposition.terms.target);
            if let Some(user) = focus {
                if owner != Some(AiAccountTarget::User(user)) {
                    continue;
                }
            }
            if let Some(AiAccountTarget::User(user)) = owner {
                linked_user_days.insert((user, *day));
            }
            let Some(complete) = account_slices.get(&(account.account_id, *day)).copied() else {
                sum.days_not_imported += 1;
                continue;
            };
            let rows = credits_by_account
                .get(&(account.account_id, *day))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let day = AccountDay {
                day: *day,
                owner,
                settled: complete && *day < settling_from,
            };
            reconcile_day(payer, &mut sum, day, rows, &tracked_by_day);
        }

        let check = checks_by_account.get(&account.account_id).copied().cloned();
        let seated = held_seat_in(account, month, today);
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
            // An interrupted check is no account's own answer: it counts
            // whatever the seat.
            if check.error.is_some() && (seated || check.interrupted) {
                flags.push(AiReconciliationFlag::ImportFailed);
                import_failed = true;
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
    for ((user, day), models) in &tracked_by_day {
        if linked_user_days.contains(&(*user, *day)) {
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
        entry.0 += weighed.map(|(_, model)| model.priced).sum::<i128>();
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
        &days,
        &coverage,
        &org_credits,
        &account_credits,
        ResidualTrust {
            settling_from,
            import_failed,
            org_check: org_check.as_ref(),
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

    let charges: Vec<&AiMeteredOrgCharge> = org_charges
        .iter()
        .filter(|charge| month.contains(charge.day))
        .collect();
    let repositories = repository_usage(&charges)?;
    let by_accounts: i128 = account_credits
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
    let month_org_credits: Vec<AiStoredOrgCredit> = org_credits
        .iter()
        .filter(|row| month.contains(row.day))
        .cloned()
        .collect();
    let pool = summary
        .map(|(lines, fetched_at)| pool_summary(&lines, fetched_at, &month_org_credits))
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

/// An imported account day.
#[derive(Clone, Copy)]
struct AccountDay {
    day: Date,
    owner: Option<AiAccountTarget>,
    /// The account's slice is complete and the day is before the settle
    /// days: the payer is done posting it.
    settled: bool,
}

/// Adds one imported account day to its sums.
fn reconcile_day(
    payer: AiProvider,
    sum: &mut AccountSum,
    AccountDay {
        day,
        owner,
        settled,
    }: AccountDay,
    rows: &[&AiStoredAccountCredit],
    tracked_by_day: &HashMap<(UserId, Date), BTreeMap<String, TrackedSum>>,
) {
    let mut owned_gross = 0_i128;
    // Day-groups of the linked user: (G, T) per (SKU, model, kind).
    let mut day_groups: BTreeMap<(Option<String>, String, AiGroupKind), (i128, i128)> =
        BTreeMap::new();
    for row in rows {
        let gross = i128::from(row.gross_usd.nanos());
        let credit = &row.credit;
        let kind = AiGroupKind::Credits(credit.kind);
        let key = (Some(credit.sku.clone()), credit.model.model.clone(), kind);
        let group = sum.groups.entry(key.clone()).or_default();
        group.labels.insert(credit.model.label.clone());
        group.gross += gross;
        if credit.model.auto_routed {
            group.auto_gross += gross;
        }
        sum.gross += gross;
        match credit.kind {
            AiCreditKind::RepositoryBilled => {
                sum.repository_billed += gross;
                continue;
            }
            AiCreditKind::Unrecognized => {
                group.unrecognized = true;
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
        day_groups.entry(key).or_default().0 += gross;
    }

    let user = match owner {
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
        Some(AiAccountTarget::User(user)) => user,
    };

    if let Some(models) = tracked_by_day.get(&(user, day)) {
        for (model, tracked) in models {
            let status = metered_model_status(payer, model);
            let trackable = day_groups.keys().find(|(_, group_model, kind)| {
                group_model == model && *kind == AiGroupKind::Credits(AiCreditKind::Trackable)
            });
            let key = match trackable {
                Some(key) => key.clone(),
                None => (None, model.clone(), AiGroupKind::TrackedOnly),
            };
            if status == Some(AiMeteredModelStatus::Free) {
                // Free models carry no weight: listed, with `T = 0`.
                sum.groups.entry(key).or_default();
                continue;
            }
            sum.tracked += tracked.priced;
            sum.tracked_old_basis += tracked.old_basis;
            sum.tracked_before_floor += tracked.before_floor;
            sum.unpriced_records += tracked.unpriced_records;
            if tracked.old_basis_buckets > 0 {
                sum.stale = true;
            }
            if tracked.priced > 0 && status.is_none() {
                sum.unknown_model = true;
            }
            day_groups.entry(key.clone()).or_default().1 += tracked.priced;
            let group = sum.groups.entry(key).or_default();
            if tracked.old_basis_buckets > 0 {
                group.stale = true;
            }
        }
    }

    for (key, (gross, tracked)) in day_groups {
        let attributed = gross.min(tracked);
        let untracked = gross - attributed;
        sum.attributed += attributed;
        sum.untracked += untracked;
        if key.2 == AiGroupKind::Credits(AiCreditKind::Trackable) {
            sum.share_denominator += gross;
        }
        let zero_gross = settled && gross == 0 && tracked > 0;
        let low_ratio = settled
            && gross > 0
            && gross * i128::from(LOW_RATIO.1) < i128::from(LOW_RATIO.0) * tracked;
        let group = sum.groups.entry(key).or_default();
        group.linked_gross += gross;
        group.tracked += tracked;
        group.attributed += attributed;
        group.untracked += untracked;
        if zero_gross {
            group.zero_gross_tracked += tracked;
            sum.zero_gross_tracked += tracked;
        }
        if low_ratio {
            group.low_ratio = true;
            sum.low_ratio = true;
        }
    }
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
struct ResidualTrust<'a> {
    /// The first day within the settle days.
    settling_from: Date,
    /// A seated account's import failed in the month, or a run stopped
    /// before finishing an account's month.
    import_failed: bool,
    org_check: Option<&'a AiUsageCheck>,
}

fn residual_days(
    days: &[Date],
    coverage: &BTreeMap<Date, AiDayCoverage>,
    org_credits: &[AiStoredOrgCredit],
    account_credits: &[AiStoredAccountCredit],
    ResidualTrust {
        settling_from,
        import_failed,
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
                || import_failed
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
