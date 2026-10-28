//! Loading a metered payer's UTC month: what reconciliation and the bill's
//! metered lines are computed from. One loader, so the reconciliation tab and
//! the bill read the same accounts, credits, coverage, tracked usage and
//! repository mappings.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        held_seat_in, utc_day, AiAccountTarget, AiBillingMonth, AiDayCoverage, AiImportStatus,
        AiMeteredSource, AiProvider, AiProviderAccountId, AiReconciliationInput,
        AiRepositoryResolver, AiSliceRecord, AiStoredProviderAccount, AiUsageCheck,
        TimeTrackingCompany, UserId,
    },
    ports::outbound::{AiMeteredUsageRepository, AiProviderAccountRepository},
    AiMeteredUsageError,
};

/// How a metered payer's months are read.
#[derive(Debug, Clone)]
pub struct AiMeteredReadSettings {
    /// The first UTC day imports cover: earlier days are not imported.
    pub backfill_from: Date,
    /// Buckets before it are never repriced.
    pub reprice_floor: OffsetDateTime,
    /// Only project and repository mappings to this company resolve, as in
    /// the bill.
    pub mapping_company: Option<TimeTrackingCompany>,
}

/// Reads a metered payer's UTC months.
pub struct AiMeteredMonths<R, A> {
    repository: Arc<R>,
    accounts: Arc<A>,
    payer: AiProvider,
    settings: AiMeteredReadSettings,
}

impl<R, A> Clone for AiMeteredMonths<R, A> {
    fn clone(&self) -> Self {
        Self {
            repository: self.repository.clone(),
            accounts: self.accounts.clone(),
            payer: self.payer,
            settings: self.settings.clone(),
        }
    }
}

/// A loaded month.
pub struct AiMeteredMonthData {
    pub now: OffsetDateTime,
    pub today: Date,
    /// Every stored account, whatever the focus.
    pub accounts: Vec<AiStoredProviderAccount>,
    pub status: AiImportStatus,
    pub input: AiReconciliationInput,
}

/// The days of `month` from `from` through `through`, both inclusive.
pub(super) fn days_between(month: AiBillingMonth, from: Date, through: Date) -> Vec<Date> {
    let mut days = Vec::new();
    let mut day = from.max(month.first_day());
    let last = through.min(month.last_day());
    while day <= last {
        days.push(day);
        match day.next_day() {
            Some(next) => day = next,
            None => break,
        }
    }
    days
}

impl<R, A> AiMeteredMonths<R, A> {
    pub fn new(
        repository: Arc<R>,
        accounts: Arc<A>,
        payer: AiProvider,
        settings: AiMeteredReadSettings,
    ) -> Self {
        Self {
            repository,
            accounts,
            payer,
            settings,
        }
    }

    /// The reconciled days of `month` and their coverage. `account_slices`
    /// holds each fetched account slice and whether it is complete;
    /// `org_slices` and `org_usage_slices` are the organization's credits and
    /// usage rows slices.
    #[allow(clippy::too_many_arguments)]
    fn coverage(
        &self,
        month: AiBillingMonth,
        today: Date,
        accounts: &[&AiStoredProviderAccount],
        account_slices: &HashMap<(AiProviderAccountId, Date), bool>,
        org_slices: &[AiSliceRecord],
        org_usage_slices: &[AiSliceRecord],
        checks: &[AiUsageCheck],
    ) -> BTreeMap<Date, AiDayCoverage> {
        // An account whose own import failed without a seat in the month is
        // not expected: its error is informational. An interrupted check is no
        // answer of the account's own, so it stays expected.
        let informational: HashSet<AiProviderAccountId> = checks
            .iter()
            .filter(|check| check.error.is_some() && !check.interrupted)
            .filter_map(|check| check.account_id)
            .filter(|account_id| {
                accounts
                    .iter()
                    .find(|account| account.account_id == *account_id)
                    .is_some_and(|account| !held_seat_in(account, month, today))
            })
            .collect();
        let expected: Vec<AiProviderAccountId> = accounts
            .iter()
            .map(|account| account.account_id)
            .filter(|account_id| !informational.contains(account_id))
            .collect();
        let by_day = |slices: &[AiSliceRecord]| -> HashMap<Date, bool> {
            slices
                .iter()
                .map(|slice| (slice.day, slice.is_complete()))
                .collect()
        };
        let org_slices = by_day(org_slices);
        let org_usage_slices = by_day(org_usage_slices);

        days_between(month, self.settings.backfill_from, today)
            .into_iter()
            .map(|day| {
                let mut coverage = AiDayCoverage {
                    org_fetched: org_slices.contains_key(&day),
                    org_complete: org_slices.get(&day).copied().unwrap_or(false),
                    org_usage_fetched: org_usage_slices.contains_key(&day),
                    org_usage_complete: org_usage_slices.get(&day).copied().unwrap_or(false),
                    ..AiDayCoverage::default()
                };
                for account_id in &expected {
                    match account_slices.get(&(*account_id, day)) {
                        None => coverage.accounts_missing += 1,
                        Some(false) => coverage.accounts_incomplete += 1,
                        Some(true) => {}
                    }
                }
                (day, coverage)
            })
            .collect()
    }
}

impl<R, A> AiMeteredMonths<R, A>
where
    R: AiMeteredUsageRepository,
    A: AiProviderAccountRepository,
{
    /// The month of everyone, or of one user's linked days: the accounts in
    /// scope, their credits, checks and slices, the coverage of the month's
    /// imported days through today, the organization's credits, rows,
    /// summary and repository mappings (not in one user's view) and the
    /// tracked usage per destination.
    pub async fn load(
        &self,
        month: AiBillingMonth,
        focus: Option<UserId>,
    ) -> Result<AiMeteredMonthData, AiMeteredUsageError> {
        let now = self.repository.now().await?;
        let today = utc_day(now);
        let accounts = self.accounts.accounts(self.payer).await?;
        let status = self.repository.import_status(self.payer).await?;
        let settle_days = status.settle_days();
        let dates = month.dates();

        let in_scope: Vec<&AiStoredProviderAccount> = match focus {
            None => accounts.iter().collect(),
            Some(user) => accounts
                .iter()
                .filter(|account| {
                    account.dispositions.iter().any(|disposition| {
                        disposition.terms.target == AiAccountTarget::User(user)
                            && disposition.terms.period.to_date_range(dates).is_some()
                    })
                })
                .collect(),
        };
        let ids: Vec<AiProviderAccountId> =
            in_scope.iter().map(|account| account.account_id).collect();
        let account_credits = self
            .repository
            .account_credits(self.payer, dates, focus.is_some().then_some(ids.as_slice()))
            .await?;
        // The running run's marks keep the checks they marked while it runs.
        let importing = status.running.as_ref().map(|run| run.id);
        let checks: Vec<AiUsageCheck> = self
            .repository
            .checks(self.payer, month, importing)
            .await?
            .into_iter()
            .filter(|check| {
                focus.is_none()
                    || check
                        .account_id
                        .is_some_and(|account_id| ids.contains(&account_id))
            })
            .collect();
        let account_slices: HashMap<(AiProviderAccountId, Date), bool> = self
            .repository
            .slices(self.payer, AiMeteredSource::AccountCredits, dates)
            .await?
            .into_iter()
            .filter_map(|slice| Some(((slice.account_id?, slice.day), slice.is_complete())))
            .collect();
        let org_slices = self
            .repository
            .slices(self.payer, AiMeteredSource::OrgCredits, dates)
            .await?;
        let org_usage_slices = self
            .repository
            .slices(self.payer, AiMeteredSource::OrgUsage, dates)
            .await?;
        let coverage = self.coverage(
            month,
            today,
            &in_scope,
            &account_slices,
            &org_slices,
            &org_usage_slices,
            &checks,
        );
        let (org_credits, org_charges, summary, repository_mappings) = match focus {
            None => (
                self.repository.org_credits(self.payer, dates).await?,
                self.repository.org_charges(self.payer, dates).await?,
                self.repository.summary(self.payer, month).await?,
                AiRepositoryResolver::new(
                    self.repository.repository_mappings(self.payer).await?,
                    self.settings.mapping_company.clone(),
                ),
            ),
            Some(_) => (
                Vec::new(),
                Vec::new(),
                None,
                AiRepositoryResolver::default(),
            ),
        };
        let tracked = self
            .repository
            .tracked_usage(
                self.payer,
                dates,
                self.settings.reprice_floor,
                self.settings.mapping_company.as_ref(),
                focus,
            )
            .await?;

        let input = AiReconciliationInput {
            payer: self.payer,
            month,
            today,
            settle_days,
            accounts: in_scope.into_iter().cloned().collect(),
            account_credits,
            org_credits,
            org_charges,
            summary,
            tracked,
            checks,
            coverage,
            account_slices,
            focus,
            repository_mappings,
        };
        Ok(AiMeteredMonthData {
            now,
            today,
            accounts,
            status,
            input,
        })
    }
}
