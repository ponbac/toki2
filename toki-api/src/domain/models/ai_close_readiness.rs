//! Whether a month's AI bill is ready to close: seven checks the server
//! evaluates whenever close is asked about or asked for.
//!
//! Everything here is pure. The billing service loads the month (its bill,
//! the metered payer's import, the uploads) and these functions judge it.
//! Each check passes, fails, or does not apply; an admin can waive a failed
//! check with a recorded reason, and the close keeps every check's problems
//! and each waiver (`readiness_json`). Anything waived wrongly is still
//! corrected later, by an adjustment. The one problem no waiver covers is a
//! live import of the month: a close is refused until it finishes. A live
//! re-import of a closed month whose adjustments post on the month blocks
//! nothing: the next open month nets what it changes after the close.
//!
//! 1. **`import`** (metered months): the payer's import is complete through
//!    the month's end, judged per account and month from the stored slices
//!    and checks, never from a run's errors (they only describe that run). No
//!    live run may be importing the month (its plan holds it; this cannot be
//!    waived), the month must be settled (its last day is before today less
//!    the measured settle days, at least 3), every day imported from every
//!    source, and the latest check of every account that held a seat, of
//!    every account a run left interrupted (whatever its seat), of every
//!    other account with credits or lines, and of the organization a success
//!    that matches the payer's month total, made once the month settled.
//!    Checks are read as the bill reads them: a mark of the live run keeps
//!    the check it marked, while one a stopped or dead run left is an
//!    interrupted check. Passing it means no metered line may still change
//!    for the import's sake.
//! 2. **`dispositions`** (metered): no credits bill to an unlinked account.
//! 3. **`unknownAccount`** (metered): the per-day residual of accounts Toki
//!    never listed is settled and zero on every day.
//! 4. **`repositories`** (metered): every repository with credits is mapped,
//!    and no organization row naming a repository is of a kind Toki does not
//!    recognize (it may be code review billed on no line).
//! 5. **`pricing`**: no metered account's lines rest on an old pricing basis
//!    or a provisional model, and no bucket the bill estimates (from the
//!    reprice floor on) is on an old basis, per machine. Closing freezes the
//!    estimates API lines bill and subscription fees are split by, which a
//!    later reprice no longer reaches.
//! 6. **`exchangeRates`**: every rate the bill converts with is stored, not
//!    being fetched and final (an admin's override is final), and so are the
//!    rates of the months its adjustment lines correct.
//! 7. **`uploads`**: every machine active in the month uploaded all of it;
//!    in metered months also the UTC hours outside the local month that the
//!    payer's UTC month holds.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use time::{Date, Duration, OffsetDateTime};

use super::{
    currencies_to_convert, held_seat_in, AiBillingDeveloper, AiBillingMonth, AiConvertedAmount,
    AiConvertedBill, AiCoverageInterval, AiCurrency, AiImportRun, AiLocalDay, AiMachineId,
    AiMeteredAccountReason, AiMeteredAllocation, AiMeteredChargeKind, AiMeteredProvisional,
    AiMeteredSource, AiMonthBill, AiMonthCompleteness, AiProvider, AiProviderAccountId,
    AiProviderLogin, AiReconciliationInput, AiUsageCheck, AiUsageDateRange, AiUsdNanos,
};

/// The longest waiver reason, in characters.
pub const MAX_WAIVER_REASON_CHARS: usize = 500;

/// The checks, in the order they are evaluated and shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AiCloseCheckId {
    Import,
    Dispositions,
    UnknownAccount,
    Repositories,
    Pricing,
    ExchangeRates,
    Uploads,
}

impl AiCloseCheckId {
    pub const ALL: [Self; 7] = [
        Self::Import,
        Self::Dispositions,
        Self::UnknownAccount,
        Self::Repositories,
        Self::Pricing,
        Self::ExchangeRates,
        Self::Uploads,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Dispositions => "dispositions",
            Self::UnknownAccount => "unknownAccount",
            Self::Repositories => "repositories",
            Self::Pricing => "pricing",
            Self::ExchangeRates => "exchangeRates",
            Self::Uploads => "uploads",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == raw)
    }
}

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCloseCheckStatus {
    Passed,
    /// It found problems; a waiver lets the month close anyway.
    Failed,
    /// It does not apply to the month (a metered check before the cutover).
    NotApplicable,
}

/// Why an admin waived a failed check: trimmed, 1 to 500 characters, with no
/// control characters other than line breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiWaiverReason(String);

impl AiWaiverReason {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let reason = raw.trim();
        let chars = reason.chars().count();
        if chars == 0 || chars > MAX_WAIVER_REASON_CHARS {
            return Err(format!(
                "a waiver needs a reason of 1 to {MAX_WAIVER_REASON_CHARS} characters"
            ));
        }
        if reason
            .chars()
            .any(|character| character.is_control() && character != '\n')
        {
            return Err("a waiver's reason must not contain control characters".to_string());
        }
        Ok(Self(reason.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An admin's waiver of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCloseWaiver {
    pub check: AiCloseCheckId,
    pub reason: AiWaiverReason,
}

/// One check's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCloseCheck {
    pub id: AiCloseCheckId,
    pub status: AiCloseCheckStatus,
    pub problems: Vec<AiCloseProblem>,
    /// Only on a failed check.
    pub waiver: Option<AiWaiverReason>,
}

impl AiCloseCheck {
    /// Failed with problems, passed without.
    fn judged(id: AiCloseCheckId, problems: Vec<AiCloseProblem>) -> Self {
        Self {
            id,
            status: if problems.is_empty() {
                AiCloseCheckStatus::Passed
            } else {
                AiCloseCheckStatus::Failed
            },
            problems,
            waiver: None,
        }
    }

    fn not_applicable(id: AiCloseCheckId) -> Self {
        Self {
            id,
            status: AiCloseCheckStatus::NotApplicable,
            problems: Vec::new(),
            waiver: None,
        }
    }

    /// Failed and waived.
    pub fn is_waived(&self) -> bool {
        self.status == AiCloseCheckStatus::Failed && self.waiver.is_some()
    }

    /// Failed, and a waiver can let the month close: none of its problems
    /// is one no waiver covers (`AiCloseProblem::waivable`).
    pub fn waivable(&self) -> bool {
        self.status == AiCloseCheckStatus::Failed
            && self.problems.iter().all(AiCloseProblem::waivable)
    }
}

/// A month's readiness to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCloseReadiness {
    pub month: AiBillingMonth,
    pub evaluated_at: OffsetDateTime,
    /// In `AiCloseCheckId::ALL` order.
    pub checks: Vec<AiCloseCheck>,
}

impl AiCloseReadiness {
    /// Every failed check is waived, and none holds a problem no waiver
    /// covers (a live import of the month).
    pub fn ready(&self) -> bool {
        self.checks.iter().all(|check| {
            check.status != AiCloseCheckStatus::Failed
                || (check.waiver.is_some() && check.waivable())
        })
    }

    /// Applies waivers to the checks that failed. A waiver of a check that
    /// did not fail, or of one holding a problem no waiver covers, is
    /// dropped, so the record never shows one.
    pub fn waive(mut self, waivers: Vec<AiCloseWaiver>) -> Self {
        for waiver in waivers {
            if let Some(check) = self
                .checks
                .iter_mut()
                .find(|check| check.id == waiver.check && check.waivable())
            {
                check.waiver = Some(waiver.reason);
            }
        }
        self
    }

    /// When the live import run importing the month started, while one is:
    /// no waiver lets the month close until it finishes.
    pub fn import_running(&self) -> Option<OffsetDateTime> {
        self.checks
            .iter()
            .flat_map(|check| &check.problems)
            .find_map(|problem| match problem {
                AiCloseProblem::ImportRunning { started_at } => Some(*started_at),
                _ => None,
            })
    }

    /// The waived checks.
    pub fn waived(&self) -> Vec<AiCloseCheckId> {
        self.checks
            .iter()
            .filter(|check| check.is_waived())
            .map(|check| check.id)
            .collect()
    }
}

/// How an account's or the organization's latest check fails the import
/// check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiCheckOutcome {
    /// Never checked against the payer's month total.
    Missing,
    /// The latest check failed: the import's own error.
    Failed(String),
    /// A run stopped, or its process died, before finishing the month, so its
    /// days may be half replaced (the check's error says why). It counts
    /// whatever the account's seat.
    Interrupted(String),
    /// The stored days differ from the payer's month total.
    Differs,
    /// Checked before the month settled: the payer may have changed it since.
    BeforeSettled { checked_at: OffsetDateTime },
}

/// What a failed check found. Every problem but `ImportRunning` can be
/// waived (`waivable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiCloseProblem {
    /// A live import run, started at `started_at`, is importing the month:
    /// its days may be half replaced until it finishes, so no waiver lets the
    /// month close meanwhile.
    ImportRunning {
        started_at: OffsetDateTime,
    },
    /// The payer may still change the month's last days.
    NotSettled {
        last_day: Date,
        settles_on: Date,
        settle_days: u32,
    },
    /// Days before the import's backfill have no imported usage.
    DaysBeforeBackfill {
        first: Date,
        last: Date,
    },
    /// Days whose slices of a source are missing or not complete.
    DaysNotImported {
        source: AiMeteredSource,
        days: u32,
        first: Date,
        last: Date,
    },
    AccountCheck {
        account_id: AiProviderAccountId,
        login: AiProviderLogin,
        outcome: AiCheckOutcome,
    },
    OrgCheck {
        outcome: AiCheckOutcome,
    },
    /// Credits on days the account has no disposition.
    UnlinkedCredits {
        account_id: AiProviderAccountId,
        login: AiProviderLogin,
        credits: i64,
    },
    /// The residual may still change.
    ResidualNotSettled {
        reasons: Vec<AiMeteredProvisional>,
    },
    /// A day's residual, signed: credits of accounts Toki never listed, or
    /// (negative) accounts above the organization.
    Residual {
        day: Date,
        nanos: i64,
    },
    UnmappedRepository {
        repository: String,
        credits: i64,
    },
    /// An organization row naming a repository is of a kind Toki does not
    /// recognize.
    UnrecognizedRepositoryRow,
    MeteredPricing {
        account_id: AiProviderAccountId,
        login: AiProviderLogin,
        reasons: Vec<AiMeteredProvisional>,
    },
    /// A machine's buckets the bill estimates are on an old pricing basis,
    /// on local days `first_day` through `last_day` (never hours: admins see
    /// days at the finest).
    OldBasis {
        developer: AiBillingDeveloper,
        machine_id: AiMachineId,
        label: String,
        provider: AiProvider,
        buckets: i64,
        first_day: Date,
        last_day: Date,
        estimate: AiUsdNanos,
    },
    RateMissing {
        currency: AiCurrency,
    },
    RatePending {
        currency: AiCurrency,
    },
    RateProvisional {
        currency: AiCurrency,
        observed_through: Date,
    },
    /// An adjustment line's corrected month has no rate for its currency.
    AdjustedRateMissing {
        month: AiBillingMonth,
        currency: AiCurrency,
    },
    AdjustedRateProvisional {
        month: AiBillingMonth,
        currency: AiCurrency,
    },
    MachineIncomplete {
        developer: AiBillingDeveloper,
        machine_id: AiMachineId,
        label: String,
        gaps: Vec<AiUsageDateRange>,
        /// Expected providers with gaps.
        providers: Vec<(AiProvider, Vec<AiUsageDateRange>)>,
    },
    /// Hours of the payer's UTC month outside the local month (`from` to
    /// `until`, month boundaries) that a machine, or one of its expected
    /// providers, has not uploaded all of.
    UtcHoursNotUploaded {
        developer: AiBillingDeveloper,
        machine_id: AiMachineId,
        label: String,
        provider: Option<AiProvider>,
        from: OffsetDateTime,
        until: OffsetDateTime,
    },
}

impl AiCloseProblem {
    /// Whether a waiver of its check can let the month close. A live import
    /// of the month cannot be waived: a close would freeze the days it may
    /// be replacing right now, half replaced. Every other problem can.
    pub fn waivable(&self) -> bool {
        !matches!(self, Self::ImportRunning { .. })
    }
}

/// A machine's buckets the local-day bill estimates that are on an old
/// pricing basis, per provider (`AiBillingRepository::old_basis_usage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiOldBasisUsage {
    pub developer: AiBillingDeveloper,
    pub machine_id: AiMachineId,
    pub label: String,
    pub provider: AiProvider,
    pub buckets: i64,
    /// The local days of the first and last such bucket.
    pub first_day: Date,
    pub last_day: Date,
    /// Their estimated cost.
    pub estimate: AiUsdNanos,
}

/// The four metered checks (import, dispositions, unknown account and
/// repositories) of a metered month, from its loaded input and allocation,
/// and the payer's live import run, if any (`AiImportStatus::running`).
pub fn metered_checks(
    input: &AiReconciliationInput,
    allocation: &AiMeteredAllocation,
    running: Option<&AiImportRun>,
) -> [AiCloseCheck; 4] {
    [
        AiCloseCheck::judged(
            AiCloseCheckId::Import,
            import_problems(input, allocation, running),
        ),
        AiCloseCheck::judged(
            AiCloseCheckId::Dispositions,
            disposition_problems(allocation),
        ),
        AiCloseCheck::judged(
            AiCloseCheckId::UnknownAccount,
            unknown_account_problems(allocation),
        ),
        AiCloseCheck::judged(
            AiCloseCheckId::Repositories,
            repository_problems(allocation),
        ),
    ]
}

/// The metered checks of a month the payer's credits do not bill.
pub fn metered_not_applicable() -> [AiCloseCheck; 4] {
    [
        AiCloseCheckId::Import,
        AiCloseCheckId::Dispositions,
        AiCloseCheckId::UnknownAccount,
        AiCloseCheckId::Repositories,
    ]
    .map(AiCloseCheck::not_applicable)
}

/// The day the month settles: its last day plus the settle days, plus one.
pub fn settles_on(month: AiBillingMonth, settle_days: u32) -> Date {
    month.last_day() + Duration::days(i64::from(settle_days) + 1)
}

fn import_problems(
    input: &AiReconciliationInput,
    allocation: &AiMeteredAllocation,
    running: Option<&AiImportRun>,
) -> Vec<AiCloseProblem> {
    let month = input.month;
    let mut problems = Vec::new();
    // A live run's marks keep the checks they marked, as the bill reads
    // them, so the run itself is the problem: it may be replacing the
    // month's days right now.
    if let Some(run) = running.filter(|run| run.months.iter().any(|(m, _)| *m == month)) {
        problems.push(AiCloseProblem::ImportRunning {
            started_at: run.started_at,
        });
    }
    let settles_on = settles_on(month, input.settle_days);
    if input.today < settles_on {
        problems.push(AiCloseProblem::NotSettled {
            last_day: month.last_day(),
            settles_on,
            settle_days: input.settle_days,
        });
    }

    let mut before_backfill = Vec::new();
    let mut missing: BTreeMap<AiMeteredSource, Vec<Date>> = BTreeMap::new();
    let mut day = month.first_day();
    while month.contains(day) && day <= input.today {
        match input.coverage.get(&day) {
            None => before_backfill.push(day),
            Some(cover) => {
                let sources = [
                    (
                        AiMeteredSource::AccountCredits,
                        cover.accounts_missing == 0 && cover.accounts_incomplete == 0,
                    ),
                    (
                        AiMeteredSource::OrgCredits,
                        cover.org_fetched && cover.org_complete,
                    ),
                    (
                        AiMeteredSource::OrgUsage,
                        cover.org_usage_fetched && cover.org_usage_complete,
                    ),
                ];
                for (source, imported) in sources {
                    if !imported {
                        missing.entry(source).or_default().push(day);
                    }
                }
            }
        }
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    if let (Some(first), Some(last)) = (before_backfill.first(), before_backfill.last()) {
        problems.push(AiCloseProblem::DaysBeforeBackfill {
            first: *first,
            last: *last,
        });
    }
    for (source, days) in missing {
        if let (Some(first), Some(last)) = (days.first(), days.last()) {
            problems.push(AiCloseProblem::DaysNotImported {
                source,
                days: u32::try_from(days.len()).unwrap_or(u32::MAX),
                first: *first,
                last: *last,
            });
        }
    }

    // Checks made before the month settled may predate the payer's last
    // changes to it.
    let settled_at = settles_on.midnight().assume_utc();
    let outcome = |check: Option<&AiUsageCheck>| -> Option<AiCheckOutcome> {
        let Some(check) = check else {
            return Some(AiCheckOutcome::Missing);
        };
        if let Some(error) = &check.error {
            return Some(if check.interrupted {
                AiCheckOutcome::Interrupted(error.clone())
            } else {
                AiCheckOutcome::Failed(error.clone())
            });
        }
        if check.matches != Some(true) {
            return Some(AiCheckOutcome::Differs);
        }
        (check.checked_at < settled_at).then_some(AiCheckOutcome::BeforeSettled {
            checked_at: check.checked_at,
        })
    };
    let with_credits: HashSet<AiProviderAccountId> = input
        .account_credits
        .iter()
        .filter(|row| month.contains(row.day))
        .map(|row| row.account_id)
        .chain(
            allocation
                .lines
                .iter()
                .filter_map(|line| line.charge.account.as_ref())
                .map(|account| account.account_id),
        )
        .collect();
    let mut accounts: Vec<_> = input.accounts.iter().collect();
    accounts
        .sort_by_cached_key(|account| (account.login.as_str().to_lowercase(), account.account_id));
    for account in accounts {
        let check = input
            .checks
            .iter()
            .find(|check| check.account_id == Some(account.account_id));
        let seated = held_seat_in(account, month, input.today);
        // A run that stopped before finishing the account's month gave no
        // answer of the account's own: its days may be half replaced, its
        // credits included, so it counts whatever the seat, as for the bill.
        let interrupted = check.is_some_and(|check| check.interrupted);
        // An import that failed on its own without a seat in the month is
        // not expected: its error is informational, as for the bill.
        let excused = !seated && !interrupted && check.is_some_and(|check| check.error.is_some());
        let expected =
            seated || interrupted || (!excused && with_credits.contains(&account.account_id));
        if !expected {
            continue;
        }
        if let Some(outcome) = outcome(check) {
            problems.push(AiCloseProblem::AccountCheck {
                account_id: account.account_id,
                login: account.login.clone(),
                outcome,
            });
        }
    }
    let org = input.checks.iter().find(|check| check.account_id.is_none());
    if let Some(outcome) = outcome(org) {
        problems.push(AiCloseProblem::OrgCheck { outcome });
    }
    problems
}

fn disposition_problems(allocation: &AiMeteredAllocation) -> Vec<AiCloseProblem> {
    let mut unlinked: BTreeMap<(String, AiProviderAccountId), (AiProviderLogin, i64)> =
        BTreeMap::new();
    for line in &allocation.lines {
        let charge = &line.charge;
        if charge.kind != AiMeteredChargeKind::Account(AiMeteredAccountReason::Unlinked)
            || charge.allocated == AiUsdNanos::ZERO
        {
            continue;
        }
        let Some(account) = &charge.account else {
            continue;
        };
        let entry = unlinked
            .entry((account.login.as_str().to_lowercase(), account.account_id))
            .or_insert_with(|| (account.login.clone(), 0));
        entry.1 = entry.1.saturating_add(charge.credits());
    }
    unlinked
        .into_iter()
        .map(
            |((_, account_id), (login, credits))| AiCloseProblem::UnlinkedCredits {
                account_id,
                login,
                credits,
            },
        )
        .collect()
}

fn unknown_account_problems(allocation: &AiMeteredAllocation) -> Vec<AiCloseProblem> {
    let mut problems = Vec::new();
    if !allocation.unknown_account.is_empty() {
        problems.push(AiCloseProblem::ResidualNotSettled {
            reasons: allocation.unknown_account.clone(),
        });
    }
    problems.extend(allocation.unknown_residual.iter().map(|(day, nanos)| {
        AiCloseProblem::Residual {
            day: *day,
            nanos: *nanos,
        }
    }));
    problems
}

fn repository_problems(allocation: &AiMeteredAllocation) -> Vec<AiCloseProblem> {
    let mut problems: Vec<AiCloseProblem> = allocation
        .unmapped_repositories()
        .into_iter()
        .map(|(repository, credits)| AiCloseProblem::UnmappedRepository {
            repository: repository.to_string(),
            credits,
        })
        .collect();
    // An organization row naming a repository under a SKU or unit Toki does
    // not know may be code review billed on no line.
    if allocation
        .repository_provisional
        .contains(&AiMeteredProvisional::Unrecognized)
    {
        problems.push(AiCloseProblem::UnrecognizedRepositoryRow);
    }
    problems
}

/// The `pricing` check: per metered account whose lines rest on an old
/// pricing basis or a provisional model (`allocation`, in metered months),
/// and per machine and provider with estimated buckets on an old basis.
/// Unrecognized models never block: the payer's credits are its own number.
pub fn pricing_check(
    allocation: Option<&AiMeteredAllocation>,
    old_basis: &[AiOldBasisUsage],
) -> AiCloseCheck {
    let mut accounts: BTreeMap<
        (String, AiProviderAccountId),
        (AiProviderLogin, BTreeSet<AiMeteredProvisional>),
    > = BTreeMap::new();
    for line in allocation.iter().flat_map(|allocation| &allocation.lines) {
        let Some(account) = &line.charge.account else {
            continue;
        };
        let reasons: Vec<AiMeteredProvisional> = line
            .charge
            .provisional
            .iter()
            .copied()
            .filter(|reason| {
                matches!(
                    reason,
                    AiMeteredProvisional::OldPricingBasis | AiMeteredProvisional::ProvisionalModel
                )
            })
            .collect();
        if reasons.is_empty() {
            continue;
        }
        accounts
            .entry((account.login.as_str().to_lowercase(), account.account_id))
            .or_insert_with(|| (account.login.clone(), BTreeSet::new()))
            .1
            .extend(reasons);
    }
    let mut problems: Vec<AiCloseProblem> = accounts
        .into_iter()
        .map(
            |((_, account_id), (login, reasons))| AiCloseProblem::MeteredPricing {
                account_id,
                login,
                reasons: reasons.into_iter().collect(),
            },
        )
        .collect();
    problems.extend(old_basis.iter().map(|usage| AiCloseProblem::OldBasis {
        developer: usage.developer.clone(),
        machine_id: usage.machine_id,
        label: usage.label.clone(),
        provider: usage.provider,
        buckets: usage.buckets,
        first_day: usage.first_day,
        last_day: usage.last_day,
        estimate: usage.estimate,
    }));
    AiCloseCheck::judged(AiCloseCheckId::Pricing, problems)
}

/// The `exchangeRates` check: every currency the bill converts has a stored
/// rate that is not being fetched and is final; every adjustment line's
/// corrected month has a final rate for its currency and a known amount in
/// the billing currency. An admin's override is final.
pub fn exchange_rate_check(bill: &AiMonthBill, converted: &AiConvertedBill) -> AiCloseCheck {
    let mut problems = Vec::new();
    for currency in currencies_to_convert(bill) {
        let rate = converted
            .rates
            .iter()
            .find(|rate| rate.currency == currency && rate.rate().is_some());
        if converted.pending_rates.contains(&currency) {
            problems.push(AiCloseProblem::RatePending { currency });
        } else if let Some(rate) = rate {
            if rate.is_provisional() {
                problems.push(AiCloseProblem::RateProvisional {
                    observed_through: rate
                        .fetched
                        .as_ref()
                        .map_or(bill.month.last_day(), |fetched| fetched.observed_through),
                    currency,
                });
            }
        } else {
            problems.push(AiCloseProblem::RateMissing { currency });
        }
    }
    let mut adjusted: BTreeSet<(Date, String, bool)> = BTreeSet::new();
    for line in &bill.adjustments {
        let month = line.adjusts.month;
        let problem = match &line.converted {
            AiConvertedAmount::MissingRate { currency } => Some((
                currency.clone(),
                AiCloseProblem::AdjustedRateMissing {
                    month,
                    currency: currency.clone(),
                },
            )),
            AiConvertedAmount::Converted {
                rate: Some(rate), ..
            } if rate.is_provisional() => Some((
                rate.currency.clone(),
                AiCloseProblem::AdjustedRateProvisional {
                    month,
                    currency: rate.currency.clone(),
                },
            )),
            _ => None,
        };
        if let Some((currency, problem)) = problem {
            let missing = matches!(problem, AiCloseProblem::AdjustedRateMissing { .. });
            if adjusted.insert((month.first_day(), currency.as_str().to_string(), missing)) {
                problems.push(problem);
            }
        }
    }
    AiCloseCheck::judged(AiCloseCheckId::ExchangeRates, problems)
}

/// The instants of `month`'s UTC month outside its local days (`days`, in
/// order): after the local month in zones east of UTC (`[local end, UTC
/// end)`), before it in zones west of UTC (`[UTC start, local start)`).
pub fn utc_spans_outside(
    month: AiBillingMonth,
    days: &[AiLocalDay],
) -> Vec<(OffsetDateTime, OffsetDateTime)> {
    let utc_start = month.first_day().midnight().assume_utc();
    let utc_end = month.dates().end().midnight().assume_utc();
    let mut spans = Vec::new();
    if let Some(first) = days.first() {
        if first.start > utc_start {
            spans.push((utc_start, first.start.min(utc_end)));
        }
    }
    if let Some(last) = days.last() {
        if last.end < utc_end {
            spans.push((last.end.max(utc_start), utc_end));
        }
    }
    spans
}

/// The first instant of `[from, until)` that `intervals` leave uncovered, or
/// `None` when they cover it all.
fn first_uncovered(
    mut intervals: Vec<(OffsetDateTime, OffsetDateTime)>,
    from: OffsetDateTime,
    until: OffsetDateTime,
) -> Option<OffsetDateTime> {
    intervals.sort();
    let mut covered = from;
    for (start, end) in intervals {
        if covered >= until {
            break;
        }
        if start > covered {
            break;
        }
        covered = covered.max(end);
    }
    (covered < until).then_some(covered)
}

/// The `uploads` check: every machine active in the month uploaded every day
/// of it (`completeness`), and in a metered month, `intervals` (logged syncs
/// of every machine) cover the UTC hours outside the local month (`spans`)
/// for the machine as a whole and, where it does, for each expected provider.
/// Inactive machines, and developers without machines, never block.
pub fn uploads_check(
    completeness: &AiMonthCompleteness,
    metered: bool,
    spans: &[(OffsetDateTime, OffsetDateTime)],
    intervals: &[(AiMachineId, AiCoverageInterval)],
) -> AiCloseCheck {
    let mut problems = Vec::new();
    for developer in &completeness.developers {
        for machine in developer
            .machines
            .iter()
            .filter(|machine| machine.active_in_month)
        {
            let id = machine.machine.machine_id;
            if !machine.complete {
                problems.push(AiCloseProblem::MachineIncomplete {
                    developer: developer.developer.clone(),
                    machine_id: id,
                    label: machine.machine.label.clone(),
                    gaps: machine.gaps.clone(),
                    providers: machine
                        .providers
                        .iter()
                        .filter(|provider| provider.expected && !provider.gaps.is_empty())
                        .map(|provider| (provider.provider, provider.gaps.clone()))
                        .collect(),
                });
            }
            if !metered {
                continue;
            }
            let logged: Vec<&AiCoverageInterval> = intervals
                .iter()
                .filter(|(machine_id, _)| *machine_id == id)
                .map(|(_, interval)| interval)
                .collect();
            for &(from, until) in spans {
                // Reports the span, never where coverage stops: that would tell
                // when the machine synced.
                let uncovered = |provider: Option<AiProvider>| {
                    let covering = logged
                        .iter()
                        .filter(|interval| provider.is_none_or(|p| interval.provider == p))
                        .map(|interval| (interval.start, interval.end))
                        .collect();
                    first_uncovered(covering, from, until).is_some()
                };
                let not_uploaded =
                    |provider: Option<AiProvider>| AiCloseProblem::UtcHoursNotUploaded {
                        developer: developer.developer.clone(),
                        machine_id: id,
                        label: machine.machine.label.clone(),
                        provider,
                        from,
                        until,
                    };
                if uncovered(None) {
                    problems.push(not_uploaded(None));
                    continue;
                }
                for provider in machine
                    .providers
                    .iter()
                    .filter(|provider| provider.expected)
                {
                    if uncovered(Some(provider.provider)) {
                        problems.push(not_uploaded(Some(provider.provider)));
                    }
                }
            }
        }
    }
    AiCloseCheck::judged(AiCloseCheckId::Uploads, problems)
}

#[cfg(test)]
mod tests;
