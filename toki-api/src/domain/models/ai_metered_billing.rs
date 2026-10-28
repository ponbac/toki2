//! The bill's lines for a metered payer (GitHub Copilot) from the cutover on:
//! what the payer billed (`G`), split by what Toki tracked (`T`). The full
//! policy (cutover, destinations, account lines, unknown account) is in
//! `.docs/ai-usage-billing.md`, "GitHub Copilot lines in the bill".
//!
//! Everything here is pure. `allocate_metered_credits` walks the month as
//! reconciliation does (`ai_metered_reconciliation`), so the bill and the
//! reconciliation tab agree on `G`, `T`, attributed and untracked for every
//! account. The invariants kept here:
//!
//! - **The T/G rule** (`split_group`), per account, UTC day and (SKU, model)
//!   group: `T ≥ G` splits `G` by each destination's `T_d`; `T < G` charges
//!   each its `T_d` and leaves `G − T` untracked; `G = 0` bills nothing. No
//!   destination is charged more than its `T_d`.
//! - **Rounding.** Per account and month, the billed `G` is rounded once to
//!   cents and split by largest remainder, so an account's lines sum exactly
//!   to its gross; a project or personal line is capped at its tracked cost
//!   rounded down, the excess cent going to the user's untracked line.
//! - **Code review** bills per repository (`repository_lines`), rounded once
//!   per repository and UTC month, never by Toki's weights.
//! - **Provisional** (`AiMeteredProvisional`): credits not imported or not
//!   computed yet are never a settled zero, for accounts, the unknown account
//!   and repositories alike.
//!
//! Amounts are nanodollars until rounding; a credit is $0.01, so a line's
//! cents are its credits.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use time::{Date, OffsetDateTime};

use super::ai_metered_reconciliation::{
    is_low_ratio, out_of_range, residual_days, walk, AccountWalk, AiMeteredWalk, ResidualTrust,
};
use super::{
    allocate_largest_remainder, metered_model_status, repository_key, AiAccountTarget,
    AiBillingMonth, AiBillingProject, AiBillingSubject, AiBillingUsage, AiCreditKind, AiCurrency,
    AiFeeAmount, AiGroupKind, AiMappedProject, AiMeteredDestination, AiMeteredModelStatus,
    AiOrgChargeKind, AiProvider, AiProviderAccountId, AiProviderLogin, AiReconciliationInput,
    AiResidualStatus, AiStoredAccountCredit, AiStoredOrgCredit, AiUnassignedKind, AiUsdNanos,
    UserId, NANOS_PER_CREDIT,
};
use crate::domain::AiMeteredUsageError;

/// From which UTC month a metered payer's usage bills from the payer's own
/// credits rather than Toki's estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiMeteredCutover {
    pub payer: AiProvider,
    pub from: AiBillingMonth,
}

impl AiMeteredCutover {
    /// `from`'s first day at 00:00 UTC: metered buckets start here.
    pub fn starts_at(&self) -> OffsetDateTime {
        self.from.first_day().midnight().assume_utc()
    }

    /// Whether month key `month` bills the payer's lines from credits, for
    /// its UTC month.
    pub fn applies_to(&self, month: AiBillingMonth) -> bool {
        month.first_day() >= self.from.first_day()
    }
}

/// The provider account a line bills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredAccountRef {
    pub account_id: AiProviderAccountId,
    pub login: AiProviderLogin,
}

/// Why a metered line bills to an account rather than a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiMeteredAccountReason {
    /// Credits on days the account has no disposition.
    Unlinked,
    /// Credits on days its disposition is External.
    External,
    /// The per-day residual: accounts Toki never listed.
    Unknown,
}

/// What a metered line charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiMeteredChargeKind {
    /// Credits attributed to a project, or to one kind of Unassigned.
    Project,
    /// Credits attributed to personal/internal usage: Spinit's.
    Personal,
    /// Credits Toki did not track, the cloud agent included: Spinit's.
    Untracked,
    /// Credits of an account no user owns: Spinit's.
    Account(AiMeteredAccountReason),
    /// Credits the payer billed to a repository (code review).
    Repository,
}

/// Why a metered line may still change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiMeteredProvisional {
    /// The UTC month has not ended.
    MonthInProgress,
    /// A day's slice is missing or not yet complete, a run stopped before
    /// finishing the account's month, or days before the import's backfill
    /// are not imported.
    ImportIncomplete,
    /// A day is within the settle days, or the stored month is not confirmed
    /// by the payer's month total (absent, failed or different).
    NotSettled,
    /// A contributing bucket is on an old pricing basis.
    OldPricingBasis,
    /// A model the catalog marks provisional contributed.
    ProvisionalModel,
    /// A credits row priced otherwise than a credit at $0.01, or a model
    /// outside the catalog, contributed; for repository credits, an
    /// organization row naming a repository is of a kind Toki does not
    /// recognize.
    Unrecognized,
}

/// A metered payer's charge, in USD at its list price of $0.01 a credit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredCharge {
    pub kind: AiMeteredChargeKind,
    /// The account billed: on every per-account line; `None` for unknown
    /// accounts and repository credits.
    pub account: Option<AiMeteredAccountRef>,
    pub sku: Option<String>,
    /// Whole cents of USD, which are the line's credits.
    pub amount: AiFeeAmount,
    /// The nanodollars allocated before rounding.
    pub allocated: AiUsdNanos,
    /// Sorted, unique; empty when settled.
    pub provisional: Vec<AiMeteredProvisional>,
    /// A settled day-group with this line's usage was billed below 85% of
    /// its tracked usage.
    pub low_ratio: bool,
}

impl AiMeteredCharge {
    /// The line's credits: one per cent.
    pub fn credits(&self) -> i64 {
        self.amount.hundredths
    }
}

/// Tracked usage of the payer on no line, by why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiMeteredUnbilled {
    /// On days no disposition links the user to an account.
    pub tracked_unlinked: AiBillingUsage,
    /// On days whose credits are not imported (before the backfill, or the
    /// linked account's slice is missing).
    pub not_imported: AiBillingUsage,
    /// Of free models, which the payer does not bill.
    pub free_models: AiBillingUsage,
}

impl AiMeteredUnbilled {
    fn add(&mut self, why: Unbilled, usage: &AiBillingUsage) -> Result<(), AiMeteredUsageError> {
        let total = match why {
            Unbilled::TrackedUnlinked => &mut self.tracked_unlinked,
            Unbilled::NotImported => &mut self.not_imported,
            Unbilled::FreeModel => &mut self.free_models,
        };
        add_usage(total, usage)
    }
}

#[derive(Clone, Copy)]
enum Unbilled {
    TrackedUnlinked,
    NotImported,
    FreeModel,
}

/// What a month's bill says about its metered lines as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredBillInfo {
    pub payer: AiProvider,
    /// Some line or account may still change, line or not, or tracked usage
    /// is not imported.
    pub provisional: bool,
    /// Account days without an imported slice (excused accounts aside).
    pub days_not_imported: u32,
    /// Settled day-groups billed below 85% of their tracked usage.
    pub low_ratio_groups: u32,
    pub unbilled: AiMeteredUnbilled,
}

/// One metered line, before it joins the bill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredLine {
    pub subject: AiBillingSubject,
    /// The project of a project line; `None` otherwise.
    pub project: Option<AiMappedProject>,
    /// The kind of Unassigned of a project line without a project.
    pub unassigned: Option<AiUnassignedKind>,
    pub charge: AiMeteredCharge,
    /// The tracked usage the line charges for: zero for untracked and
    /// account lines.
    pub usage: AiBillingUsage,
}

/// A metered payer's lines for one UTC month.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMeteredAllocation {
    pub payer: AiProvider,
    pub month: AiBillingMonth,
    /// By account (login, then id), then kind, destination and SKU; then the
    /// unknown account, then repositories by name, case-insensitively.
    pub lines: Vec<AiMeteredLine>,
    /// Why the unknown account's residual may still change, sorted and
    /// unique; empty when it is settled. Kept whether or not the residual
    /// bills a line, and empty in one user's part, which has no such line.
    pub unknown_account: Vec<AiMeteredProvisional>,
    /// The residual's settled and provisional days that are not zero, code
    /// review left out on both sides, by day, signed: what the unknown
    /// account bills (positive), or accounts above the organization
    /// (negative). Empty in one user's part.
    pub unknown_residual: Vec<(Date, i64)>,
    /// Why repository credits may still change, sorted and unique; empty when
    /// they are settled. Kept with or without repository lines, and empty in
    /// one user's part, which has none.
    pub repository_provisional: Vec<AiMeteredProvisional>,
    pub unbilled: AiMeteredUnbilled,
    pub unbilled_by_user: BTreeMap<i32, AiMeteredUnbilled>,
    pub days_not_imported: u32,
    pub low_ratio_groups: u32,
    pub provisional: bool,
    pub month_in_progress: bool,
    /// Per user: days not imported, low-ratio groups.
    user_counters: BTreeMap<i32, (u32, u32)>,
    /// Users linked on a day of the month to an account that may still
    /// change, whether or not they have a line of it.
    provisional_users: BTreeSet<i32>,
}

impl AiMeteredAllocation {
    /// The ids of every user whose part (`for_user`) differs from an empty
    /// one: those with lines, with tracked usage on no line, with days not
    /// imported or low-ratio groups, or linked to an account that may still
    /// change, which keeps their part provisional without a line.
    pub fn user_ids(&self) -> BTreeSet<i32> {
        self.lines
            .iter()
            .filter_map(|line| line.subject.user_id().map(|user| user.as_i32()))
            .chain(self.unbilled_by_user.keys().copied())
            .chain(self.user_counters.keys().copied())
            .chain(self.provisional_users.iter().copied())
            .collect()
    }

    /// Repositories whose credits bill as Unassigned, without a resolving
    /// repository mapping, with their credits: the readiness check
    /// `repositories` refuses them.
    pub fn unmapped_repositories(&self) -> Vec<(&str, i64)> {
        self.lines
            .iter()
            .filter(|line| line.unassigned == Some(AiUnassignedKind::UnmappedRepository))
            .filter_map(|line| match &line.subject {
                AiBillingSubject::Repository { name, .. } => {
                    Some((name.as_str(), line.charge.credits()))
                }
                _ => None,
            })
            .collect()
    }

    pub fn info(&self) -> AiMeteredBillInfo {
        AiMeteredBillInfo {
            payer: self.payer,
            provisional: self.provisional,
            days_not_imported: self.days_not_imported,
            low_ratio_groups: self.low_ratio_groups,
            unbilled: self.unbilled,
        }
    }
}

/// Kinds of sink, in the order an account's cents are split over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum SinkKind {
    Project,
    Personal,
    Untracked,
    Unlinked,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SinkKey {
    kind: SinkKind,
    user: Option<i32>,
    /// Project and personal sinks: the destination's identity.
    destination: Option<(u8, String)>,
    /// Untracked, unlinked and external sinks: the credits' SKU.
    sku: Option<String>,
}

/// Where an account's credits go before rounding.
#[derive(Debug, Clone)]
struct Sink {
    kind: SinkKind,
    user: Option<UserId>,
    destination: Option<AiMeteredDestination>,
    sku: Option<String>,
    allocated: i128,
    /// Project and personal sinks: `Σ T_d` over groups with credits, the
    /// most they may bill.
    cap: i128,
    usage: AiBillingUsage,
    low_ratio: bool,
}

impl Sink {
    fn new(key: &SinkKey, user: Option<UserId>, destination: Option<AiMeteredDestination>) -> Self {
        Self {
            kind: key.kind,
            user,
            destination,
            sku: key.sku.clone(),
            allocated: 0,
            cap: 0,
            usage: AiBillingUsage::default(),
            low_ratio: false,
        }
    }

    fn order_key(&self) -> (SinkKind, (u8, String, String), i32, Option<String>) {
        (
            self.kind,
            self.destination
                .as_ref()
                .map(AiMeteredDestination::order_key)
                .unwrap_or_default(),
            self.user.map_or(0, |user| user.as_i32()),
            self.sku.clone(),
        )
    }
}

fn nanos(value: i128) -> Result<AiUsdNanos, AiMeteredUsageError> {
    i64::try_from(value)
        .ok()
        .and_then(AiUsdNanos::new)
        .ok_or_else(out_of_range)
}

fn add_usage(
    total: &mut AiBillingUsage,
    usage: &AiBillingUsage,
) -> Result<(), AiMeteredUsageError> {
    total.add(usage).map_err(|_| out_of_range())
}

fn usd_cents(hundredths: i64) -> AiFeeAmount {
    AiFeeAmount {
        hundredths,
        currency: AiCurrency::usd(),
    }
}

/// Allocates a UTC month of a metered payer's credits to the bill's lines;
/// see the module documentation. `input` must cover every account
/// (`focus: None`): the developer page takes its lines with `for_user`, so it
/// agrees with the overview to the cent.
pub fn allocate_metered_credits(
    input: &AiReconciliationInput,
) -> Result<AiMeteredAllocation, AiMeteredUsageError> {
    if input.focus.is_some() {
        return Err(AiMeteredUsageError::Invalid(
            "metered credits are allocated over every account".to_string(),
        ));
    }
    let payer = input.payer;
    let walk = walk(input)?;
    let month_in_progress = input.today <= input.month.last_day();
    // Days of the month before the import's backfill have no credits at all.
    let expected_days = (input.today.min(input.month.last_day()) - input.month.first_day())
        .whole_days()
        .saturating_add(1)
        .max(0);
    let coverage_gap = i64::try_from(walk.days.len()).unwrap_or(i64::MAX) < expected_days;

    let mut lines = Vec::new();
    let mut user_counters: BTreeMap<i32, (u32, u32)> = BTreeMap::new();
    let mut provisional_users: BTreeSet<i32> = BTreeSet::new();
    let mut provisional_accounts = false;
    let mut days_not_imported = 0_u32;
    let mut low_ratio_groups = 0_u32;
    // Whether the account linked to a user on a day has its slice.
    let mut linked_slices: HashMap<(UserId, Date), bool> = HashMap::new();

    let mut walks: Vec<&AccountWalk<'_>> = walk.accounts.iter().collect();
    walks.sort_by_cached_key(|account| {
        (
            account.account.login.as_str().to_lowercase(),
            account.account.account_id,
        )
    });
    for account_walk in walks {
        let account = account_walk.account;
        let mut reasons: BTreeSet<AiMeteredProvisional> = BTreeSet::new();
        if month_in_progress {
            reasons.insert(AiMeteredProvisional::MonthInProgress);
        }
        if coverage_gap {
            reasons.insert(AiMeteredProvisional::ImportIncomplete);
        }
        if !account_walk.excused {
            let confirmed = account_walk
                .check
                .is_some_and(|check| check.error.is_none() && check.matches == Some(true));
            if !confirmed {
                reasons.insert(AiMeteredProvisional::NotSettled);
            }
        }
        // A run stopped before finishing the account's month: its days may be
        // half replaced, whatever their slices say.
        if account_walk.check.is_some_and(|check| check.interrupted) {
            reasons.insert(AiMeteredProvisional::ImportIncomplete);
        }

        let mut sinks: HashMap<SinkKey, Sink> = HashMap::new();
        // The account's gross and its code review, to check that every other
        // credit reached a sink.
        let (mut gross_sum, mut repository_billed) = (0_i128, 0_i128);
        for day in &account_walk.days {
            if let Some(AiAccountTarget::User(user)) = day.owner {
                *linked_slices.entry((user, day.day)).or_default() |= day.slice.is_some();
            }
            let Some(complete) = day.slice else {
                if !account_walk.excused {
                    days_not_imported += 1;
                    reasons.insert(AiMeteredProvisional::ImportIncomplete);
                    if let Some(AiAccountTarget::User(user)) = day.owner {
                        user_counters.entry(user.as_i32()).or_default().0 += 1;
                    }
                }
                continue;
            };
            if !complete && !account_walk.excused {
                reasons.insert(AiMeteredProvisional::ImportIncomplete);
            }
            if day.day >= walk.settling_from {
                reasons.insert(AiMeteredProvisional::NotSettled);
            }

            for group in &day.groups {
                let gross = group.gross;
                gross_sum += gross;
                let status = metered_model_status(payer, group.model());
                if group
                    .rows
                    .iter()
                    .any(|row| row.credit.kind == AiCreditKind::Unrecognized)
                    || (group.kind() == AiGroupKind::Credits(AiCreditKind::Trackable)
                        && gross > 0
                        && status.is_none())
                {
                    reasons.insert(AiMeteredProvisional::Unrecognized);
                }
                if group.kind() == AiGroupKind::Credits(AiCreditKind::RepositoryBilled) {
                    repository_billed += gross;
                    continue;
                }
                let user = match day.owner {
                    None => {
                        account_sink(&mut sinks, SinkKind::Unlinked, group.sku()).allocated +=
                            gross;
                        continue;
                    }
                    Some(AiAccountTarget::External) => {
                        account_sink(&mut sinks, SinkKind::External, group.sku()).allocated +=
                            gross;
                        continue;
                    }
                    Some(AiAccountTarget::User(user)) => user,
                };

                let tracked = group.tracked_cost();
                let low_ratio = day.settled && is_low_ratio(gross, tracked);
                if low_ratio {
                    low_ratio_groups += 1;
                    user_counters.entry(user.as_i32()).or_default().1 += 1;
                }
                for ((destination, sum), charge) in group.tracked.iter().zip(&group.charges) {
                    let kind = match destination {
                        AiMeteredDestination::Personal => SinkKind::Personal,
                        AiMeteredDestination::Project(_) => SinkKind::Project,
                    };
                    let key = SinkKey {
                        kind,
                        user: Some(user.as_i32()),
                        destination: Some(destination.identity()),
                        sku: None,
                    };
                    let sink = sinks
                        .entry(key.clone())
                        .or_insert_with(|| Sink::new(&key, Some(user), Some(destination.clone())));
                    sink.allocated += charge;
                    if gross > 0 {
                        sink.cap += sum.priced;
                        if sink.sku.is_none() {
                            sink.sku = group.sku().cloned();
                        }
                        if sum.old_basis_buckets > 0 {
                            reasons.insert(AiMeteredProvisional::OldPricingBasis);
                        }
                        if sum.priced > 0 {
                            match status {
                                Some(AiMeteredModelStatus::Provisional) => {
                                    reasons.insert(AiMeteredProvisional::ProvisionalModel);
                                }
                                None => {
                                    reasons.insert(AiMeteredProvisional::Unrecognized);
                                }
                                Some(_) => {}
                            }
                        }
                    }
                    if low_ratio && sum.priced > 0 {
                        sink.low_ratio = true;
                    }
                    add_usage(&mut sink.usage, &sum.usage()?)?;
                }
                if group.untracked > 0 {
                    let key = SinkKey {
                        kind: SinkKind::Untracked,
                        user: Some(user.as_i32()),
                        destination: None,
                        sku: group.sku().cloned(),
                    };
                    sinks
                        .entry(key.clone())
                        .or_insert_with(|| Sink::new(&key, Some(user), None))
                        .allocated += group.untracked;
                }
            }
        }

        let billed: i128 = sinks.values().map(|sink| sink.allocated).sum();
        if billed != gross_sum - repository_billed {
            return Err(AiMeteredUsageError::Storage(format!(
                "the metered allocation of {} lost credits",
                account.login
            )));
        }
        let cents = nanos(billed)?.to_cents();
        let mut ordered: Vec<Sink> = sinks.into_values().collect();
        ordered.sort_by_cached_key(Sink::order_key);
        let weights = ordered
            .iter()
            .map(|sink| u128::try_from(sink.allocated).map_err(|_| out_of_range()))
            .collect::<Result<Vec<_>, _>>()?;
        let shares = allocate_largest_remainder(cents, &weights)
            .map_err(|_| out_of_range())?
            .unwrap_or_else(|| vec![0; ordered.len()]);
        let mut billed_sinks: Vec<(Sink, i64)> = ordered.into_iter().zip(shares).collect();
        cap_destination_sinks(&mut billed_sinks)?;

        // The account's reasons hold for the whole bill and for every user
        // linked to it on a day of the month (before the import's backfill
        // included), as they do for its lines, whether or not it bills them
        // one: credits neither imported nor tracked yet are no settled zero.
        if !reasons.is_empty() {
            provisional_accounts = true;
            provisional_users.extend(account.dispositions.iter().filter_map(|disposition| {
                let user = disposition.terms.target.user_id()?;
                disposition
                    .terms
                    .period
                    .to_date_range(input.month.dates())
                    .map(|_| user.as_i32())
            }));
        }

        let account_ref = AiMeteredAccountRef {
            account_id: account.account_id,
            login: account.login.clone(),
        };
        let provisional: Vec<AiMeteredProvisional> = reasons.into_iter().collect();
        for (sink, sink_cents) in billed_sinks {
            if sink.allocated == 0 && sink_cents == 0 && sink.usage == AiBillingUsage::default() {
                continue;
            }
            lines.push(sink_line(
                payer,
                &account_ref,
                sink,
                sink_cents,
                provisional.clone(),
            )?);
        }
    }

    let unknown = unknown_account(input, &walk, month_in_progress, coverage_gap)?;
    lines.extend(unknown.line);
    let (repository_lines, repository_provisional) =
        repository_lines(input, &walk, month_in_progress, coverage_gap)?;
    lines.extend(repository_lines);

    // Every tracked row of the month lands in exactly one place: a line's
    // usage, or one of the unbilled reasons.
    let day_set: HashSet<Date> = walk.days.iter().copied().collect();
    let mut unbilled = AiMeteredUnbilled::default();
    let mut unbilled_by_user: BTreeMap<i32, AiMeteredUnbilled> = BTreeMap::new();
    for row in input
        .tracked
        .iter()
        .filter(|row| input.month.contains(row.day))
    {
        let why = if metered_model_status(payer, &row.model) == Some(AiMeteredModelStatus::Free) {
            Unbilled::FreeModel
        } else if !day_set.contains(&row.day) {
            Unbilled::NotImported
        } else if !walk.linked_user_days.contains(&(row.user_id, row.day)) {
            Unbilled::TrackedUnlinked
        } else if !linked_slices
            .get(&(row.user_id, row.day))
            .copied()
            .unwrap_or(false)
        {
            Unbilled::NotImported
        } else {
            continue;
        };
        let usage = row.usage();
        unbilled.add(why, &usage)?;
        unbilled_by_user
            .entry(row.user_id.as_i32())
            .or_default()
            .add(why, &usage)?;
    }

    let provisional = month_in_progress
        || days_not_imported > 0
        || unbilled.not_imported != AiBillingUsage::default()
        || provisional_accounts
        || !unknown.provisional.is_empty()
        || !repository_provisional.is_empty()
        || lines.iter().any(|line| !line.charge.provisional.is_empty());
    Ok(AiMeteredAllocation {
        payer,
        month: input.month,
        lines,
        unknown_account: unknown.provisional,
        unknown_residual: unknown.residual_days,
        repository_provisional,
        unbilled,
        unbilled_by_user,
        days_not_imported,
        low_ratio_groups,
        provisional,
        month_in_progress,
        user_counters,
        provisional_users,
    })
}

/// The unlinked or external sink of an account for credits of `sku`.
fn account_sink<'a>(
    sinks: &'a mut HashMap<SinkKey, Sink>,
    kind: SinkKind,
    sku: Option<&String>,
) -> &'a mut Sink {
    let key = SinkKey {
        kind,
        user: None,
        destination: None,
        sku: sku.cloned(),
    };
    sinks
        .entry(key.clone())
        .or_insert_with(|| Sink::new(&key, None, None))
}

/// Caps every destination (project and personal) sink at its tracked cost of
/// groups with credits, rounded down to cents, and moves the excess to the
/// same user's untracked sink of the destination's SKU, adding one if needed.
/// The cents' sum is kept.
fn cap_destination_sinks(sinks: &mut Vec<(Sink, i64)>) -> Result<(), AiMeteredUsageError> {
    let mut moved: Vec<(Option<UserId>, Option<String>, i64)> = Vec::new();
    for (sink, cents) in sinks.iter_mut() {
        if !matches!(sink.kind, SinkKind::Project | SinkKind::Personal) {
            continue;
        }
        let cap =
            i64::try_from(sink.cap / i128::from(NANOS_PER_CREDIT)).map_err(|_| out_of_range())?;
        if *cents > cap {
            moved.push((sink.user, sink.sku.clone(), *cents - cap));
            *cents = cap;
        }
    }
    if moved.is_empty() {
        return Ok(());
    }
    for (user, sku, excess) in moved {
        let found = sinks.iter_mut().find(|(sink, _)| {
            sink.kind == SinkKind::Untracked && sink.user == user && sink.sku == sku
        });
        match found {
            Some((_, cents)) => *cents += excess,
            None => {
                let key = SinkKey {
                    kind: SinkKind::Untracked,
                    user: user.map(|user| user.as_i32()),
                    destination: None,
                    sku,
                };
                sinks.push((Sink::new(&key, user, None), excess));
            }
        }
    }
    sinks.sort_by_cached_key(|(sink, _)| sink.order_key());
    Ok(())
}

fn sink_line(
    payer: AiProvider,
    account: &AiMeteredAccountRef,
    sink: Sink,
    cents: i64,
    provisional: Vec<AiMeteredProvisional>,
) -> Result<AiMeteredLine, AiMeteredUsageError> {
    let user_subject = || {
        sink.user
            .map(AiBillingSubject::User)
            .ok_or_else(out_of_range)
    };
    let account_subject = || AiBillingSubject::ProviderAccount {
        provider: payer,
        account_id: account.account_id,
        login: account.login.clone(),
    };
    let (subject, kind, project, unassigned) = match (sink.kind, &sink.destination) {
        (
            SinkKind::Project,
            Some(AiMeteredDestination::Project(AiBillingProject::Mapped(project))),
        ) => (
            user_subject()?,
            AiMeteredChargeKind::Project,
            Some(project.clone()),
            None,
        ),
        (
            SinkKind::Project,
            Some(AiMeteredDestination::Project(AiBillingProject::Unassigned(kind))),
        ) => (
            user_subject()?,
            AiMeteredChargeKind::Project,
            None,
            Some(*kind),
        ),
        (SinkKind::Project, _) | (SinkKind::Personal, _) => {
            (user_subject()?, AiMeteredChargeKind::Personal, None, None)
        }
        (SinkKind::Untracked, _) => (user_subject()?, AiMeteredChargeKind::Untracked, None, None),
        (SinkKind::Unlinked, _) => (
            account_subject(),
            AiMeteredChargeKind::Account(AiMeteredAccountReason::Unlinked),
            None,
            None,
        ),
        (SinkKind::External, _) => (
            account_subject(),
            AiMeteredChargeKind::Account(AiMeteredAccountReason::External),
            None,
            None,
        ),
    };
    Ok(AiMeteredLine {
        subject,
        project,
        unassigned,
        charge: AiMeteredCharge {
            kind,
            account: Some(account.clone()),
            sku: sink.sku,
            amount: usd_cents(cents),
            allocated: nanos(sink.allocated)?,
            provisional,
            low_ratio: sink.low_ratio,
        },
        usage: sink.usage,
    })
}

/// The unknown account's month: its line, and why its residual may still
/// change, with or without a line.
struct UnknownAccount {
    line: Option<AiMeteredLine>,
    /// Sorted, unique; empty when settled.
    provisional: Vec<AiMeteredProvisional>,
    /// Settled and provisional days with a residual, signed.
    residual_days: Vec<(Date, i64)>,
}

/// The month's residual of non-repository credits over settled and
/// provisional days, as one line when it is positive. Days that are an import
/// gap (an organization slice missing, an account's slice missing or its
/// check failed, the organization's check failed) are left out of it and make
/// it provisional, as do days that may still change, line or not.
fn unknown_account(
    input: &AiReconciliationInput,
    walk: &AiMeteredWalk<'_>,
    month_in_progress: bool,
    coverage_gap: bool,
) -> Result<UnknownAccount, AiMeteredUsageError> {
    // Code review is billed per repository: left out on both sides.
    let org: Vec<AiStoredOrgCredit> = input
        .org_credits
        .iter()
        .filter(|row| row.credit.kind != AiCreditKind::RepositoryBilled)
        .cloned()
        .collect();
    let accounts: Vec<AiStoredAccountCredit> = input
        .account_credits
        .iter()
        .filter(|row| row.credit.kind != AiCreditKind::RepositoryBilled)
        .cloned()
        .collect();
    let days = residual_days(
        &walk.days,
        &input.coverage,
        &org,
        &accounts,
        ResidualTrust {
            settling_from: walk.settling_from,
            import_failed: walk.import_failed,
            org_check: walk.org_check,
        },
    )?;
    let mut residual = 0_i128;
    let mut reasons: BTreeSet<AiMeteredProvisional> = BTreeSet::new();
    if month_in_progress {
        reasons.insert(AiMeteredProvisional::MonthInProgress);
    }
    if coverage_gap {
        reasons.insert(AiMeteredProvisional::ImportIncomplete);
    }
    let mut residual_days = Vec::new();
    for day in &days {
        match day.status {
            AiResidualStatus::Settled => residual += i128::from(day.residual),
            AiResidualStatus::Provisional => {
                residual += i128::from(day.residual);
                reasons.insert(AiMeteredProvisional::NotSettled);
            }
            AiResidualStatus::Incomplete => {
                reasons.insert(AiMeteredProvisional::ImportIncomplete);
                continue;
            }
        }
        if day.residual != 0 {
            residual_days.push((day.day, day.residual));
        }
    }
    let provisional: Vec<AiMeteredProvisional> = reasons.into_iter().collect();
    if residual <= 0 {
        return Ok(UnknownAccount {
            line: None,
            provisional,
            residual_days,
        });
    }
    let allocated = nanos(residual)?;
    let line = AiMeteredLine {
        subject: AiBillingSubject::UnknownAccount {
            provider: input.payer,
        },
        project: None,
        unassigned: None,
        charge: AiMeteredCharge {
            kind: AiMeteredChargeKind::Account(AiMeteredAccountReason::Unknown),
            account: None,
            sku: None,
            amount: usd_cents(allocated.to_cents()),
            allocated,
            provisional: provisional.clone(),
            low_ratio: false,
        },
        usage: AiBillingUsage::default(),
    };
    Ok(UnknownAccount {
        line: Some(line),
        provisional,
        residual_days,
    })
}

/// The month's repository lines: per repository (by `repository_key`), the
/// payer's repository credit rows of the UTC month summed and rounded half up
/// to cents once, to the repository's mapped project or to Unassigned
/// (unmapped repository); a repository whose credits round to none gets no
/// line, as the mapping overview marks it (`AiRepositoryMonthCredits`). Only
/// `RepositoryCredits` rows bill: the IDE aggregate, the cloud agent's rows,
/// Actions and unrecognized rows never do. Also why repository credits may
/// still change, line or not: the month in progress, a day without the
/// organization's usage slice or with an incomplete one (or days before the
/// backfill), days within the settle days, and an unrecognized row naming a
/// repository (code review under a SKU or unit Toki does not know bills
/// nowhere, so it is flagged, never dropped). They rest on the
/// organization's usage slices alone, which a run replaces a month at a time,
/// so no check, failed or interrupted, bears on them: a month a run stopped
/// importing keeps the bill provisional through the unknown account's
/// residual instead.
fn repository_lines(
    input: &AiReconciliationInput,
    walk: &AiMeteredWalk<'_>,
    month_in_progress: bool,
    coverage_gap: bool,
) -> Result<(Vec<AiMeteredLine>, Vec<AiMeteredProvisional>), AiMeteredUsageError> {
    let mut reasons: BTreeSet<AiMeteredProvisional> = BTreeSet::new();
    if month_in_progress {
        reasons.insert(AiMeteredProvisional::MonthInProgress);
    }
    let missing_usage = walk.days.iter().any(|day| {
        let cover = input.coverage.get(day).copied().unwrap_or_default();
        !cover.org_usage_fetched || !cover.org_usage_complete
    });
    if coverage_gap || missing_usage {
        reasons.insert(AiMeteredProvisional::ImportIncomplete);
    }
    if walk.days.iter().any(|day| *day >= walk.settling_from) {
        reasons.insert(AiMeteredProvisional::NotSettled);
    }
    if input.org_charges.iter().any(|charge| {
        charge.kind == AiOrgChargeKind::Unrecognized
            && !charge.repository.is_empty()
            && input.month.contains(charge.day)
    }) {
        reasons.insert(AiMeteredProvisional::Unrecognized);
    }
    let provisional: Vec<AiMeteredProvisional> = reasons.into_iter().collect();

    /// A repository's month: its latest (day, spelling), smallest SKU and
    /// credits in nanodollars.
    struct Repository<'a> {
        latest: (Date, &'a str),
        sku: &'a str,
        nanos: i128,
    }
    let mut repositories: BTreeMap<String, Repository<'_>> = BTreeMap::new();
    for charge in input.org_charges.iter().filter(|charge| {
        charge.kind == AiOrgChargeKind::RepositoryCredits && input.month.contains(charge.day)
    }) {
        let nanos = i128::from(charge.credit_usd().ok_or_else(out_of_range)?.nanos());
        let latest = (charge.day, charge.repository.as_str());
        let repository = repositories
            .entry(repository_key(&charge.repository))
            .or_insert(Repository {
                latest,
                sku: &charge.sku,
                nanos: 0,
            });
        repository.latest = repository.latest.max(latest);
        repository.sku = repository.sku.min(charge.sku.as_str());
        repository.nanos += nanos;
    }

    let mut lines = Vec::new();
    for repository in repositories.into_values() {
        let allocated = nanos(repository.nanos)?;
        let cents = allocated.to_cents();
        if cents == 0 {
            continue;
        }
        let name = repository.latest.1.to_string();
        let project = input.repository_mappings.project(&name).cloned();
        lines.push(AiMeteredLine {
            subject: AiBillingSubject::Repository {
                provider: input.payer,
                name,
            },
            unassigned: project
                .is_none()
                .then_some(AiUnassignedKind::UnmappedRepository),
            project,
            charge: AiMeteredCharge {
                kind: AiMeteredChargeKind::Repository,
                account: None,
                sku: Some(repository.sku.to_string()),
                amount: usd_cents(cents),
                allocated,
                provisional: provisional.clone(),
                low_ratio: false,
            },
            usage: AiBillingUsage::default(),
        });
    }
    Ok((lines, provisional))
}

/// One user's part of an allocation: their lines, unbilled usage and
/// counters. It is provisional as the whole is, over what is theirs: a day of
/// theirs not imported, their tracked usage not imported, or an account
/// linked to them in the month that may still change, whether or not they
/// have a line of it. The unknown account and repositories are no user's, so
/// their credits leave their part settled.
pub fn for_user(allocation: AiMeteredAllocation, user: UserId) -> AiMeteredAllocation {
    let id = user.as_i32();
    let lines: Vec<AiMeteredLine> = allocation
        .lines
        .into_iter()
        .filter(|line| line.subject == AiBillingSubject::User(user))
        .collect();
    let unbilled = allocation
        .unbilled_by_user
        .get(&id)
        .copied()
        .unwrap_or_default();
    let (days_not_imported, low_ratio_groups) = allocation
        .user_counters
        .get(&id)
        .copied()
        .unwrap_or_default();
    let account_provisional = allocation.provisional_users.contains(&id);
    let provisional = allocation.month_in_progress
        || days_not_imported > 0
        || unbilled.not_imported != AiBillingUsage::default()
        || account_provisional
        || lines.iter().any(|line| !line.charge.provisional.is_empty());
    AiMeteredAllocation {
        payer: allocation.payer,
        month: allocation.month,
        lines,
        unknown_account: Vec::new(),
        unknown_residual: Vec::new(),
        repository_provisional: Vec::new(),
        unbilled,
        unbilled_by_user: BTreeMap::from([(id, unbilled)]),
        days_not_imported,
        low_ratio_groups,
        provisional,
        month_in_progress: allocation.month_in_progress,
        user_counters: BTreeMap::from([(id, (days_not_imported, low_ratio_groups))]),
        provisional_users: if account_provisional {
            BTreeSet::from([id])
        } else {
            BTreeSet::new()
        },
    }
}

#[cfg(test)]
mod tests;
