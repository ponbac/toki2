//! A metered payer's billed usage: importing it from the payer's usage source
//! and reconciling it with Toki's tracked usage.
//!
//! - **Locks.** One import runs at a time: an in-process lock, then the
//!   store's import lock, which also keeps other instances out. Holding the
//!   store's lock proves no other run is alive, so starting a run marks any
//!   leftover `running` run interrupted.
//! - **A run** imports its months oldest first. Per month: the organization's
//!   credits, its usage rows (one call for the month), the month's summary,
//!   then every stored account's credits, three accounts at a time. Credits
//!   follow the month mode (`AiMonthMode`); an account whose month total is
//!   empty is proved empty for every day without day calls. After each scope
//!   the stored month is checked against the payer's month total.
//! - **Marks.** Before touching a month's days, a run marks every stored
//!   check of the month with its id; each scope's new check clears its mark.
//!   A check a run left marked, because it stopped or its process died before
//!   finishing that scope, reads as interrupted once the run is no longer
//!   importing: no check from before vouches for days the run may have half
//!   replaced, or for an account's days against the organization's refreshed
//!   ones. An interrupted check counts whatever the account's seat, and its
//!   month joins the next runs until one finishes it.
//! - **Errors.** A rate limit, rejected credentials, a refusal, any
//!   organization-scope error, a storage error, three `Unavailable` answers in
//!   a row or `MAX_REQUESTS_PER_RUN` stop the run, which keeps the reason; the
//!   checks it left marked read as interrupted for that reason. Other account
//!   errors are that account's check error; the other accounts continue.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use futures::{stream, StreamExt};
use time::{Date, OffsetDateTime};

use super::ai_metered_months::{
    days_between, AiMeteredMonthData, AiMeteredMonths, AiMeteredReadSettings,
};
use crate::domain::{
    models::{
        admin_plan, credit_totals, credit_totals_match, ensure_importable, gap_months,
        held_seat_in, import_freshness, imported_through, open_days, reconcile, scheduled_plan,
        settle_lag_days, total_gross_quantity, utc_day, AiAccountImportError, AiBillingMonth,
        AiCreditStoreScope, AiCreditTotal, AiExpectedSlices, AiFreshnessInput, AiImportFreshness,
        AiImportOverview, AiImportPlan, AiImportRun, AiImportRunId, AiImportStatus,
        AiMeteredCredit, AiMeteredMonth, AiMeteredPeriod, AiMeteredScope, AiMeteredSource,
        AiMeteredSourceHealth, AiMonthMode, AiProvider, AiProviderAccountId, AiProviderLogin,
        AiRunStatus, AiSliceRecord, AiSliceSet, AiSourceOutcome, AiSourceStatus,
        AiStoredProviderAccount, AiStoredScope, AiUsageCheck, AiUsageDateRange,
        TimeTrackingCompany, UserId, MAX_REQUESTS_PER_RUN, SETTLE_LAG_WINDOW_DAYS,
    },
    ports::{
        inbound::{AiImportTicket, AiMeteredUsageService},
        outbound::{
            AiImportLease, AiMeteredUsageRepository, AiProviderAccountRepository,
            MeteredUsageSource,
        },
    },
    AiMeteredUsageError, MeteredUsageSourceError,
};

/// How many accounts a run imports at once.
const IMPORT_CONCURRENCY: usize = 3;
/// `Unavailable` answers in a row that stop a run.
const MAX_CONSECUTIVE_UNAVAILABLE: u32 = 3;
/// The longest stored error, in characters.
const MAX_STORED_ERROR_CHARS: usize = 2000;

/// Whether the payer's usage source can be read.
pub enum AiMeteredSourceState {
    Ready(Arc<dyn MeteredUsageSource>),
    /// No credentials are configured.
    Disabled,
    /// Credentials are configured but unusable, for the given reason.
    Misconfigured(String),
}

/// How imports run.
#[derive(Debug, Clone, Copy)]
pub struct AiMeteredImportSettings {
    /// The first UTC day imports cover.
    pub backfill_from: Date,
    /// Buckets before it are never repriced.
    pub reprice_floor: OffsetDateTime,
    /// Whether scheduled runs happen in this build (release builds only).
    pub unattended: bool,
}

pub struct AiMeteredUsageServiceImpl<R, A> {
    repository: Arc<R>,
    accounts: Arc<A>,
    source: AiMeteredSourceState,
    payer: AiProvider,
    namespace: String,
    settings: AiMeteredImportSettings,
    /// Reads months as the bill reads them.
    months: AiMeteredMonths<R, A>,
    single_flight: Arc<tokio::sync::Mutex<()>>,
}

impl<R, A> AiMeteredUsageServiceImpl<R, A> {
    /// `payer` is whose usage this is, such as Copilot, and `namespace` where
    /// its accounts live, such as the GitHub organization. Tracked usage
    /// resolves project keys with mappings to `mapping_company`, as the bill
    /// does.
    pub fn new(
        repository: Arc<R>,
        accounts: Arc<A>,
        source: AiMeteredSourceState,
        payer: AiProvider,
        namespace: String,
        settings: AiMeteredImportSettings,
        mapping_company: Option<TimeTrackingCompany>,
    ) -> Self {
        let months = AiMeteredMonths::new(
            repository.clone(),
            accounts.clone(),
            payer,
            AiMeteredReadSettings {
                backfill_from: settings.backfill_from,
                reprice_floor: settings.reprice_floor,
                mapping_company,
            },
        );
        Self {
            repository,
            accounts,
            source,
            payer,
            namespace,
            settings,
            months,
            single_flight: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn ready_source(&self) -> Result<Arc<dyn MeteredUsageSource>, AiMeteredUsageError> {
        match &self.source {
            AiMeteredSourceState::Ready(source) => Ok(source.clone()),
            AiMeteredSourceState::Disabled => Err(AiMeteredUsageError::SourceOff),
            AiMeteredSourceState::Misconfigured(message) => {
                Err(AiMeteredUsageError::SourceMisconfigured(message.clone()))
            }
        }
    }

    fn health(&self) -> AiMeteredSourceHealth {
        match &self.source {
            AiMeteredSourceState::Ready(_) => AiMeteredSourceHealth::Ready,
            AiMeteredSourceState::Disabled => AiMeteredSourceHealth::Disabled,
            AiMeteredSourceState::Misconfigured(message) => {
                AiMeteredSourceHealth::Misconfigured(message.clone())
            }
        }
    }
}

fn clip_error(error: &str) -> String {
    let error = error.trim();
    let error = if error.is_empty() {
        "unknown error"
    } else {
        error
    };
    error.chars().take(MAX_STORED_ERROR_CHARS).collect()
}

fn range_through(first: Date, last: Date) -> Result<AiUsageDateRange, AiMeteredUsageError> {
    AiUsageDateRange::from_inclusive(first, last)
        .ok_or_else(|| AiMeteredUsageError::Invalid(format!("no days from {first} to {last}")))
}

/// Why a step of a run did not finish.
enum Failure {
    /// The run stopped earlier; nothing was attempted.
    Stopped,
    Source(MeteredUsageSourceError),
    Storage(AiMeteredUsageError),
}

impl From<AiMeteredUsageError> for Failure {
    fn from(error: AiMeteredUsageError) -> Self {
        Self::Storage(error)
    }
}

/// What a run did for one source.
#[derive(Default)]
struct Tally {
    planned: u32,
    completed: u32,
    failed: u32,
    requests: u32,
    slices_fetched: u32,
    slices_changed: u32,
    error: Option<String>,
}

/// One run in progress.
struct Run<'a, R> {
    id: AiImportRunId,
    repository: &'a R,
    source: &'a dyn MeteredUsageSource,
    payer: AiProvider,
    today: Date,
    /// The first UTC day imported; earlier days are never fetched.
    backfill_from: Date,
    settle_days: u32,
    requests: AtomicU32,
    consecutive_unavailable: AtomicU32,
    account_errors: AtomicU32,
    stop: Mutex<Option<String>>,
    tallies: Mutex<HashMap<AiMeteredSource, Tally>>,
}

impl<R: AiMeteredUsageRepository> Run<'_, R> {
    fn stopped(&self) -> bool {
        self.stop
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Stops the run for `reason`, kept on `source`; the first reason wins.
    fn halt(&self, source: AiMeteredSource, reason: String) {
        let reason = clip_error(&reason);
        self.tally(source, |tally| {
            tally.failed += 1;
            tally.error.get_or_insert_with(|| reason.clone());
        });
        self.stop
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert(reason);
    }

    fn tally(&self, source: AiMeteredSource, update: impl FnOnce(&mut Tally)) {
        let mut tallies = self.tallies.lock().unwrap_or_else(|e| e.into_inner());
        update(tallies.entry(source).or_default());
    }

    /// Sends one request, unless the run stopped or reached its cap.
    async fn call<T>(
        &self,
        source: AiMeteredSource,
        request: impl std::future::Future<Output = Result<T, MeteredUsageSourceError>>,
    ) -> Result<T, Failure> {
        if self.stopped() {
            return Err(Failure::Stopped);
        }
        if self.requests.fetch_add(1, Ordering::SeqCst) >= MAX_REQUESTS_PER_RUN {
            self.halt(
                source,
                format!("the run reached its cap of {MAX_REQUESTS_PER_RUN} requests"),
            );
            return Err(Failure::Stopped);
        }
        self.tally(source, |tally| tally.requests += 1);
        match request.await {
            Ok(value) => {
                self.consecutive_unavailable.store(0, Ordering::SeqCst);
                Ok(value)
            }
            Err(error) => Err(Failure::Source(error)),
        }
    }

    /// Writes fetched days of a credits scope.
    async fn write_days(
        &self,
        source: AiMeteredSource,
        scope: AiCreditStoreScope<'_>,
        days: &[(Date, Vec<AiMeteredCredit>)],
        fetched_at: OffsetDateTime,
    ) -> Result<(), Failure> {
        if days.is_empty() {
            return Ok(());
        }
        let changed = self
            .repository
            .replace_credit_days(self.payer, scope, days, fetched_at)
            .await?;
        let fetched = u32::try_from(days.len()).unwrap_or(u32::MAX);
        self.tally(source, |tally| {
            tally.slices_fetched += fetched;
            tally.slices_changed += changed;
        });
        Ok(())
    }

    /// Fetches and stores `days` of a credits scope, one day at a time.
    async fn fetch_days(
        &self,
        source: AiMeteredSource,
        scope: AiCreditStoreScope<'_>,
        metered: AiMeteredScope<'_>,
        days: &[Date],
        done: &mut HashSet<Date>,
    ) -> Result<(), Failure> {
        for day in days {
            if done.contains(day) {
                continue;
            }
            let fetched_at = self.repository.now().await?;
            let rows = self
                .call(
                    source,
                    self.source.credits(metered, AiMeteredPeriod::Day(*day)),
                )
                .await?;
            self.write_days(source, scope, &[(*day, rows)], fetched_at)
                .await?;
            done.insert(*day);
        }
        Ok(())
    }

    async fn stored_totals(
        &self,
        scope: AiStoredScope,
        month: AiBillingMonth,
    ) -> Result<Vec<AiCreditTotal>, Failure> {
        Ok(self
            .repository
            .credit_month_totals(self.payer, scope, month)
            .await?)
    }

    /// Imports one scope's credits of `month` in `mode` and checks the stored
    /// month against the payer's month total.
    async fn import_credits(
        &self,
        source: AiMeteredSource,
        scope: AiCreditStoreScope<'_>,
        metered: AiMeteredScope<'_>,
        month: AiBillingMonth,
        mode: AiMonthMode,
    ) -> Result<AiUsageCheck, Failure> {
        let stored_scope = scope.stored();
        let days = days_between(month, self.backfill_from, self.today);
        let fetched_at = self.repository.now().await?;
        let aggregate = self
            .call(
                source,
                self.source.credits(metered, AiMeteredPeriod::Month(month)),
            )
            .await?;
        let aggregate = credit_totals(&aggregate).ok_or_else(|| {
            Failure::Source(MeteredUsageSourceError::Response(
                "the month total is out of range".to_string(),
            ))
        })?;

        if aggregate.is_empty() {
            // The month total proves every day empty.
            let empty: Vec<(Date, Vec<AiMeteredCredit>)> =
                days.iter().map(|day| (*day, Vec::new())).collect();
            self.write_days(source, scope, &empty, fetched_at).await?;
        } else {
            let mut done = HashSet::new();
            match mode {
                AiMonthMode::Full => {
                    self.fetch_days(source, scope, metered, &days, &mut done)
                        .await?;
                }
                AiMonthMode::Changes => {
                    let range = month.dates();
                    let fetched: HashSet<Date> = self
                        .repository
                        .slices(self.payer, source, range)
                        .await?
                        .into_iter()
                        .filter(|slice| slice.account_id == stored_scope.account_id())
                        .map(|slice| slice.day)
                        .collect();
                    let missing: Vec<Date> = days
                        .iter()
                        .copied()
                        .filter(|day| !fetched.contains(day))
                        .collect();
                    self.fetch_days(source, scope, metered, &missing, &mut done)
                        .await?;
                    let stored = self.stored_totals(stored_scope, month).await?;
                    if !credit_totals_match(&aggregate, &stored) {
                        let open: Vec<Date> = open_days(month, self.today, self.settle_days)
                            .into_iter()
                            .filter(|day| *day >= self.backfill_from)
                            .collect();
                        self.fetch_days(source, scope, metered, &open, &mut done)
                            .await?;
                        let stored = self.stored_totals(stored_scope, month).await?;
                        if !credit_totals_match(&aggregate, &stored) {
                            self.fetch_days(source, scope, metered, &days, &mut done)
                                .await?;
                        }
                    }
                }
            }
        }

        let stored = self.stored_totals(stored_scope, month).await?;
        let check = AiUsageCheck {
            account_id: stored_scope.account_id(),
            checked_at: self.repository.now().await?,
            payer_gross: Some(total_gross_quantity(&aggregate).ok_or_else(out_of_range)?),
            stored_gross: total_gross_quantity(&stored).ok_or_else(out_of_range)?,
            matches: Some(credit_totals_match(&aggregate, &stored)),
            error: None,
            interrupted: false,
        };
        self.repository
            .record_check(self.payer, source, month, &check)
            .await?;
        Ok(check)
    }

    /// Imports one organization step; any failure stops the run.
    async fn org_step(
        &self,
        source: AiMeteredSource,
        step: impl std::future::Future<Output = Result<(), Failure>>,
    ) {
        if self.stopped() {
            return;
        }
        match step.await {
            Ok(()) => self.tally(source, |tally| tally.completed += 1),
            Err(Failure::Stopped) => {}
            Err(Failure::Source(error)) => self.halt(source, error.to_string()),
            Err(Failure::Storage(error)) => self.halt(source, error.to_string()),
        }
    }

    async fn import_org_usage(&self, month: AiBillingMonth) -> Result<(), Failure> {
        let source = AiMeteredSource::OrgUsage;
        let fetched_at = self.repository.now().await?;
        let rows = self
            .call(source, self.source.org_usage(AiMeteredPeriod::Month(month)))
            .await?;
        // Days from the backfill on, through today or the latest row.
        let first = month.first_day().max(self.backfill_from);
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|row| month.contains(row.day) && row.day >= first)
            .collect();
        let last = rows
            .iter()
            .map(|row| row.day)
            .fold(self.today.min(month.last_day()), Date::max);
        let days = range_through(first, last)?;
        let changed = self
            .repository
            .replace_org_usage(self.payer, days, &rows, fetched_at)
            .await?;
        let fetched = u32::try_from((days.end() - days.start()).whole_days()).unwrap_or(u32::MAX);
        self.tally(source, |tally| {
            tally.slices_fetched += fetched;
            tally.slices_changed += changed;
        });
        Ok(())
    }

    async fn import_summary(&self, month: AiBillingMonth) -> Result<(), Failure> {
        let source = AiMeteredSource::Summary;
        let fetched_at = self.repository.now().await?;
        let rows = self.call(source, self.source.summary(month)).await?;
        let changed = self
            .repository
            .replace_summary(self.payer, month, &rows, fetched_at)
            .await?;
        self.tally(source, |tally| {
            tally.slices_fetched += 1;
            tally.slices_changed += u32::from(changed);
        });
        Ok(())
    }

    /// Imports one account's month. Errors that concern only the account
    /// become its check error. One that stops the run, or a stop before or
    /// during the import, leaves the run's mark on the check: it reads as
    /// interrupted, for the run's reason.
    async fn import_account(
        &self,
        account_id: AiProviderAccountId,
        login: &AiProviderLogin,
        month: AiBillingMonth,
        mode: AiMonthMode,
    ) {
        let source = AiMeteredSource::AccountCredits;
        if self.stopped() {
            return;
        }
        let result = self
            .import_credits(
                source,
                AiCreditStoreScope::Account {
                    id: account_id,
                    login,
                },
                AiMeteredScope::Account(login),
                month,
                mode,
            )
            .await;
        let error = match result {
            Ok(_) => {
                self.tally(source, |tally| tally.completed += 1);
                return;
            }
            Err(Failure::Stopped) => return,
            Err(Failure::Storage(error)) => {
                self.halt(source, error.to_string());
                return;
            }
            Err(Failure::Source(error)) => error,
        };
        match error {
            MeteredUsageSourceError::RateLimited { .. }
            | MeteredUsageSourceError::Unauthorized(_)
            | MeteredUsageSourceError::Forbidden(_) => {
                self.halt(source, error.to_string());
                return;
            }
            MeteredUsageSourceError::Unavailable(_) => {
                let in_a_row = self.consecutive_unavailable.fetch_add(1, Ordering::SeqCst) + 1;
                if in_a_row >= MAX_CONSECUTIVE_UNAVAILABLE {
                    self.halt(source, error.to_string());
                    return;
                }
            }
            MeteredUsageSourceError::NotFound(_) | MeteredUsageSourceError::Response(_) => {}
        }

        self.account_errors.fetch_add(1, Ordering::SeqCst);
        self.tally(source, |tally| tally.failed += 1);
        let recorded = self
            .record_failed_check(
                source,
                AiStoredScope::Account(account_id),
                month,
                &error.to_string(),
            )
            .await;
        if let Err(storage) = recorded {
            self.halt(source, storage.to_string());
        }
    }

    /// Records that a scope's month could not be checked against the payer's
    /// month total, replacing its last check: the stored days are no longer
    /// confirmed.
    async fn record_failed_check(
        &self,
        source: AiMeteredSource,
        scope: AiStoredScope,
        month: AiBillingMonth,
        error: &str,
    ) -> Result<(), AiMeteredUsageError> {
        let stored = self
            .repository
            .credit_month_totals(self.payer, scope, month)
            .await?;
        let check = AiUsageCheck {
            account_id: scope.account_id(),
            checked_at: self.repository.now().await?,
            payer_gross: None,
            stored_gross: total_gross_quantity(&stored).ok_or_else(out_of_range)?,
            matches: None,
            error: Some(clip_error(error)),
            interrupted: false,
        };
        self.repository
            .record_check(self.payer, source, month, &check)
            .await
    }

    /// Imports the organization's credits of `month`. Any failure stops the
    /// run, leaving the run's mark on the organization's check, so the
    /// residual never rests on days the run may have left half replaced.
    async fn import_org_credits(
        &self,
        month: AiBillingMonth,
        mode: AiMonthMode,
    ) -> Result<(), Failure> {
        self.import_credits(
            AiMeteredSource::OrgCredits,
            AiCreditStoreScope::Organization,
            AiMeteredScope::Organization,
            month,
            mode,
        )
        .await
        .map(|_| ())
    }

    /// Marks the month's checks, before any of its days are touched; a month
    /// whose checks could not be marked is not imported.
    async fn mark_checks(&self, month: AiBillingMonth) -> bool {
        let marked = self
            .repository
            .mark_checks_importing(self.payer, month, self.id)
            .await;
        if let Err(error) = marked {
            self.halt(AiMeteredSource::OrgCredits, error.to_string());
            return false;
        }
        true
    }

    async fn run_month(
        &self,
        month: AiBillingMonth,
        mode: AiMonthMode,
        accounts: &[(AiProviderAccountId, AiProviderLogin)],
    ) {
        if !self.mark_checks(month).await {
            return;
        }
        self.org_step(
            AiMeteredSource::OrgCredits,
            self.import_org_credits(month, mode),
        )
        .await;
        self.org_step(AiMeteredSource::OrgUsage, self.import_org_usage(month))
            .await;
        self.org_step(AiMeteredSource::Summary, self.import_summary(month))
            .await;
        let imports: Vec<_> = accounts
            .iter()
            .map(|(account_id, login)| self.import_account(*account_id, login, month, mode))
            .collect();
        stream::iter(imports)
            .buffer_unordered(IMPORT_CONCURRENCY)
            .collect::<Vec<()>>()
            .await;
    }
}

/// What the latest checks mean for planning and coverage.
#[derive(Default)]
struct CheckedMonths {
    /// Accounts whose own import of a month failed although they held no
    /// seat in it (departed people, say): their slices of that month are not
    /// expected, as in `coverage`, so their errors never make the import
    /// stale or the month a gap. An interrupted check excuses no one.
    excused: Vec<(AiProviderAccountId, AiBillingMonth)>,
    /// Months holding an interrupted check of the organization or a stored
    /// account: a run stopped before finishing them, so they join the next
    /// runs until one does.
    interrupted: Vec<AiBillingMonth>,
}

fn out_of_range() -> AiMeteredUsageError {
    AiMeteredUsageError::Storage("stored quantities exceed the supported range".to_string())
}

impl<R, A> AiMeteredUsageServiceImpl<R, A>
where
    R: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
{
    async fn today(&self) -> Result<(OffsetDateTime, Date), AiMeteredUsageError> {
        let now = self.repository.now().await?;
        Ok((now, utc_day(now)))
    }

    /// Every stored slice from `backfill_from` through `today`.
    async fn all_slices(&self, today: Date) -> Result<Vec<AiSliceRecord>, AiMeteredUsageError> {
        let Some(range) = AiUsageDateRange::from_inclusive(
            AiBillingMonth::containing(self.settings.backfill_from)
                .map(|month| month.first_day())
                .unwrap_or(self.settings.backfill_from),
            today,
        ) else {
            return Ok(Vec::new());
        };
        let mut slices = Vec::new();
        for source in AiMeteredSource::ALL {
            slices.extend(self.repository.slices(self.payer, source, range).await?);
        }
        Ok(slices)
    }

    /// What every month's latest checks mean for planning and coverage. The
    /// caller holds the import lock outside of any month's import (its own
    /// run, if any, is done importing), so every mark left counts.
    async fn checked_months(
        &self,
        accounts: &[AiStoredProviderAccount],
        today: Date,
    ) -> Result<CheckedMonths, AiMeteredUsageError> {
        let mut checked = CheckedMonths::default();
        for month in self.repository.months(self.payer).await? {
            for check in self.repository.checks(self.payer, month, None).await? {
                // Checks of accounts no longer stored are never imported again.
                let account = match check.account_id {
                    None => None,
                    Some(account_id) => {
                        match accounts
                            .iter()
                            .find(|account| account.account_id == account_id)
                        {
                            Some(account) => Some(account),
                            None => continue,
                        }
                    }
                };
                if check.interrupted {
                    if !checked.interrupted.contains(&month) {
                        checked.interrupted.push(month);
                    }
                } else if let Some(account) = account {
                    if check.error.is_some() && !held_seat_in(account, month, today) {
                        checked.excused.push((account.account_id, month));
                    }
                }
            }
        }
        Ok(checked)
    }

    /// The slices an import should hold through `today`, but those of the
    /// `excused` accounts' months.
    fn expected_slices(
        &self,
        accounts: &[AiStoredProviderAccount],
        today: Date,
        excused: Vec<(AiProviderAccountId, AiBillingMonth)>,
    ) -> AiExpectedSlices {
        let ids: Vec<AiProviderAccountId> =
            accounts.iter().map(|account| account.account_id).collect();
        AiExpectedSlices::new(&ids, self.settings.backfill_from, today).excusing(excused)
    }

    /// Months holding an expected slice never fetched, or an interrupted
    /// check, oldest first.
    async fn gaps(&self, today: Date) -> Result<Vec<AiBillingMonth>, AiMeteredUsageError> {
        let accounts = self.accounts.accounts(self.payer).await?;
        let checked = self.checked_months(&accounts, today).await?;
        let expected = self.expected_slices(&accounts, today, checked.excused);
        let slices = self.all_slices(today).await?;
        let mut gaps = gap_months(&expected, &AiSliceSet::fetched(&slices));
        for month in checked.interrupted {
            if !gaps.contains(&month) {
                gaps.push(month);
            }
        }
        gaps.sort_by_key(|month| month.first_day());
        Ok(gaps)
    }

    /// Records a run of `plan` as running now.
    async fn start_run(
        &self,
        plan: &AiImportPlan,
    ) -> Result<(AiImportRunId, OffsetDateTime), AiMeteredUsageError> {
        let started_at = self.repository.now().await?;
        let run_id = self
            .repository
            .start_run(self.payer, plan, started_at)
            .await?;
        Ok((run_id, started_at))
    }

    /// Releases a lease after a failure before a run started.
    async fn abandon<T>(
        lease: Box<dyn AiImportLease>,
        error: AiMeteredUsageError,
    ) -> Result<T, AiMeteredUsageError> {
        if let Err(release) = lease.release().await {
            tracing::warn!(error = %release, "could not release the metered usage import lock");
        }
        Err(error)
    }

    /// Runs a plan and stores its outcome. Only storage errors around the run
    /// itself are returned; the run's own failures are its status.
    async fn execute(
        &self,
        run_id: AiImportRunId,
        started_at: OffsetDateTime,
        plan: &AiImportPlan,
        source: &dyn MeteredUsageSource,
    ) -> Result<AiImportRun, AiMeteredUsageError> {
        let (_, today) = self.today().await?;
        let status = self.repository.import_status(self.payer).await?;
        let stored_accounts = self.accounts.accounts(self.payer).await?;
        let accounts: Vec<(AiProviderAccountId, AiProviderLogin)> = stored_accounts
            .iter()
            .map(|account| (account.account_id, account.login.clone()))
            .collect();
        let run = Run {
            id: run_id,
            repository: self.repository.as_ref(),
            source,
            payer: self.payer,
            today,
            backfill_from: self.settings.backfill_from,
            settle_days: status.settle_days(),
            requests: AtomicU32::new(0),
            consecutive_unavailable: AtomicU32::new(0),
            account_errors: AtomicU32::new(0),
            stop: Mutex::new(None),
            tallies: Mutex::new(HashMap::new()),
        };
        let months = u32::try_from(plan.months.len()).unwrap_or(u32::MAX);
        for source in AiMeteredSource::ALL {
            let planned = match source {
                AiMeteredSource::AccountCredits => {
                    months.saturating_mul(u32::try_from(accounts.len()).unwrap_or(u32::MAX))
                }
                AiMeteredSource::OrgCredits
                | AiMeteredSource::OrgUsage
                | AiMeteredSource::Summary => months,
            };
            run.tally(source, |tally| tally.planned = planned);
        }
        for (month, mode) in &plan.months {
            if run.stopped() {
                break;
            }
            run.run_month(*month, *mode, &accounts).await;
        }

        // Coverage after the run, from the stored slices.
        let checked = self.checked_months(&stored_accounts, today).await?;
        let expected = self.expected_slices(&stored_accounts, today, checked.excused);
        let slices = self.all_slices(today).await?;
        let complete = AiSliceSet::complete(&slices);
        let stop = run.stop.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let account_errors = run.account_errors.load(Ordering::SeqCst);
        let tallies = std::mem::take(&mut *run.tallies.lock().unwrap_or_else(|e| e.into_inner()));
        let lag_from = today - time::Duration::days(SETTLE_LAG_WINDOW_DAYS);
        let sources: Vec<AiSourceOutcome> = AiMeteredSource::ALL
            .into_iter()
            .map(|source| {
                let tally = tallies.get(&source);
                let get = |field: fn(&Tally) -> u32| tally.map(field).unwrap_or(0);
                let (planned, completed, failed) =
                    (get(|t| t.planned), get(|t| t.completed), get(|t| t.failed));
                let status = if completed >= planned {
                    AiSourceStatus::Succeeded
                } else if completed == 0 && failed == 0 {
                    AiSourceStatus::Skipped
                } else if completed == 0 {
                    AiSourceStatus::Failed
                } else {
                    AiSourceStatus::Partial
                };
                let mut error = tally.and_then(|tally| tally.error.clone());
                if error.is_none()
                    && source == AiMeteredSource::AccountCredits
                    && account_errors > 0
                {
                    error = Some(format!("{account_errors} account imports failed"));
                }
                let source_slices: Vec<AiSliceRecord> = slices
                    .iter()
                    .filter(|slice| slice.source == source && slice.day >= lag_from)
                    .cloned()
                    .collect();
                AiSourceOutcome {
                    source,
                    status,
                    requests: get(|t| t.requests),
                    slices_fetched: get(|t| t.slices_fetched),
                    slices_changed: get(|t| t.slices_changed),
                    imported_through: imported_through(source, &expected, &complete, today),
                    settle_lag_days: settle_lag_days(&source_slices, today),
                    error,
                }
            })
            .collect();
        let any_completed = sources.iter().any(|outcome| {
            matches!(
                outcome.status,
                AiSourceStatus::Succeeded | AiSourceStatus::Partial
            ) && tallies
                .get(&outcome.source)
                .is_some_and(|tally| tally.completed > 0)
        });
        let status = match (&stop, account_errors) {
            (Some(_), _) if any_completed => AiRunStatus::Partial,
            (Some(_), _) => AiRunStatus::Failed,
            (None, 0) => AiRunStatus::Succeeded,
            (None, _) => AiRunStatus::Partial,
        };
        let finished_at = self.repository.now().await?.max(started_at);
        let run = AiImportRun {
            id: run_id,
            kind: plan.kind,
            months: plan.months.clone(),
            started_at,
            finished_at: Some(finished_at),
            status,
            requests: run
                .requests
                .load(Ordering::SeqCst)
                .min(MAX_REQUESTS_PER_RUN),
            slices_fetched: sources.iter().map(|s| s.slices_fetched).sum(),
            slices_changed: sources.iter().map(|s| s.slices_changed).sum(),
            account_errors,
            error: stop,
            sources,
        };
        self.repository.finish_run(self.payer, &run).await?;
        tracing::info!(
            payer = self.payer.as_str(),
            kind = run.kind.as_str(),
            months = ?run.months.iter().map(|(month, mode)| format!("{month} {}", mode.as_str())).collect::<Vec<_>>(),
            requests = run.requests,
            slices_fetched = run.slices_fetched,
            slices_changed = run.slices_changed,
            account_errors = run.account_errors,
            status = run.status.as_str(),
            error = ?run.error,
            "imported metered usage"
        );
        Ok(run)
    }

    async fn import_overview(
        &self,
        now: OffsetDateTime,
        accounts: &[AiStoredProviderAccount],
        status: AiImportStatus,
    ) -> Result<AiImportOverview, AiMeteredUsageError> {
        let today = utc_day(now);
        let account_errors: Vec<AiAccountImportError> = status
            .account_errors
            .iter()
            .filter_map(|error| {
                let account = accounts
                    .iter()
                    .find(|account| account.account_id == error.account_id)?;
                Some(AiAccountImportError {
                    account_id: error.account_id,
                    login: account.login.clone(),
                    month: error.month,
                    error: error.error.clone(),
                    seated: held_seat_in(account, error.month, today),
                    interrupted: error.interrupted,
                })
            })
            .collect();
        let summary_fetched_at = match AiBillingMonth::containing(today) {
            Some(month) => self
                .repository
                .slices(
                    self.payer,
                    AiMeteredSource::Summary,
                    range_through(month.first_day(), month.first_day())?,
                )
                .await?
                .into_iter()
                .map(|slice| slice.fetched_at)
                .max(),
            None => None,
        };
        let freshness = import_freshness(&AiFreshnessInput {
            source_ready: matches!(self.source, AiMeteredSourceState::Ready(_)),
            now,
            history: status.history,
            latest_run: status.last_run.clone(),
            imported_through: status
                .sources
                .iter()
                .map(|state| (state.source, state.imported_through))
                .collect(),
            summary_fetched_at,
            account_errors: account_errors.clone(),
        });
        Ok(AiImportOverview {
            health: self.health(),
            backfill_from: self.settings.backfill_from,
            unattended: self.settings.unattended,
            freshness,
            running: status.running,
            last_run: status.last_run,
            account_errors,
            sources: status.sources,
        })
    }

    /// The month, of everyone or of one user's linked days.
    async fn month(
        &self,
        month: AiBillingMonth,
        focus: Option<UserId>,
    ) -> Result<AiMeteredMonth, AiMeteredUsageError> {
        let AiMeteredMonthData {
            now,
            today,
            accounts,
            status,
            input,
        } = self.months.load(month, focus).await?;
        let reconciliation = reconcile(input)?;
        let months = self.repository.months(self.payer).await?;
        let import = self.import_overview(now, &accounts, status).await?;
        Ok(AiMeteredMonth {
            payer: self.payer,
            namespace: self.namespace.clone(),
            today,
            months,
            import,
            reconciliation,
        })
    }
}

#[async_trait]
impl<R, A> AiMeteredUsageService for AiMeteredUsageServiceImpl<R, A>
where
    R: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
{
    async fn reconciliation(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMeteredMonth, AiMeteredUsageError> {
        self.month(month, None).await
    }

    async fn developer_reconciliation(
        &self,
        month: AiBillingMonth,
        user: UserId,
    ) -> Result<AiMeteredMonth, AiMeteredUsageError> {
        let users = self.accounts.users().await?;
        if !users.iter().any(|candidate| candidate.user_id == user) {
            return Err(AiMeteredUsageError::UserNotFound);
        }
        self.month(month, Some(user)).await
    }

    async fn begin_import(
        &self,
        month: AiBillingMonth,
        mode: AiMonthMode,
        by: UserId,
    ) -> Result<AiImportTicket, AiMeteredUsageError> {
        self.ready_source()?;
        let (_, today) = self.today().await?;
        ensure_importable(month, today, self.settings.backfill_from)?;
        let guard = self
            .single_flight
            .clone()
            .try_lock_owned()
            .map_err(|_| AiMeteredUsageError::ImportRunning)?;
        let lease = self
            .repository
            .try_lock_import(self.payer)
            .await?
            .ok_or(AiMeteredUsageError::ImportRunning)?;
        let plan =
            match self.gaps(today).await.and_then(|gaps| {
                admin_plan(month, mode, today, self.settings.backfill_from, &gaps, by)
            }) {
                Ok(plan) => plan,
                Err(error) => return Self::abandon(lease, error).await,
            };
        let (run_id, started_at) = match self.start_run(&plan).await {
            Ok(started) => started,
            Err(error) => return Self::abandon(lease, error).await,
        };
        Ok(AiImportTicket {
            run_id,
            started_at,
            plan,
            lease,
            guard,
        })
    }

    async fn run_import(&self, ticket: AiImportTicket) -> Result<AiImportRun, AiMeteredUsageError> {
        let AiImportTicket {
            run_id,
            started_at,
            plan,
            lease,
            guard,
        } = ticket;
        let result = match self.ready_source() {
            Ok(source) => {
                self.execute(run_id, started_at, &plan, source.as_ref())
                    .await
            }
            Err(error) => Err(error),
        };
        let released = lease.release().await;
        drop(guard);
        let run = result?;
        released?;
        Ok(run)
    }

    async fn import_if_due(&self) -> Result<Option<AiImportRun>, AiMeteredUsageError> {
        self.ready_source()?;
        let Ok(guard) = self.single_flight.clone().try_lock_owned() else {
            return Ok(None);
        };
        let Some(lease) = self.repository.try_lock_import(self.payer).await? else {
            return Ok(None);
        };
        let planned = async {
            let (now, today) = self.today().await?;
            let status = self.repository.import_status(self.payer).await?;
            let gaps = self.gaps(today).await?;
            Ok::<_, AiMeteredUsageError>(scheduled_plan(
                now,
                self.settings.backfill_from,
                &status.history,
                &gaps,
                status.settle_days(),
            ))
        };
        let plan = match planned.await {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                lease.release().await?;
                return Ok(None);
            }
            Err(error) => return Self::abandon(lease, error).await,
        };
        let (run_id, started_at) = match self.start_run(&plan).await {
            Ok(started) => started,
            Err(error) => return Self::abandon(lease, error).await,
        };
        self.run_import(AiImportTicket {
            run_id,
            started_at,
            plan,
            lease,
            guard,
        })
        .await
        .map(Some)
    }

    async fn freshness(&self) -> Result<AiImportFreshness, AiMeteredUsageError> {
        let (now, _) = self.today().await?;
        let accounts = self.accounts.accounts(self.payer).await?;
        let status = self.repository.import_status(self.payer).await?;
        Ok(self
            .import_overview(now, &accounts, status)
            .await?
            .freshness)
    }
}
