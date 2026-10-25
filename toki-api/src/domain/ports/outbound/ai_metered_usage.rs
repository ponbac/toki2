use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AiBillingMonth, AiCreditStoreScope, AiCreditTotal, AiImportPlan, AiImportRun,
        AiImportRunId, AiImportStatus, AiMeteredCredit, AiMeteredOrgCharge, AiMeteredPeriod,
        AiMeteredScope, AiMeteredSource, AiMeteredSummaryLine, AiProvider, AiProviderAccountId,
        AiSliceRecord, AiStoredAccountCredit, AiStoredOrgCredit, AiStoredScope, AiTrackedModelDay,
        AiUsageCheck, AiUsageDateRange, TimeTrackingCompany, UserId,
    },
    AiMeteredUsageError, MeteredUsageSourceError,
};

/// What a metered payer billed, read from the payer. For Copilot: GitHub's
/// billing usage API of the organization.
#[async_trait]
pub trait MeteredUsageSource: Send + Sync + 'static {
    /// Credits per product, SKU and model label of one account or the
    /// organization, over a UTC month (summed) or one UTC day. Keys never
    /// repeat.
    async fn credits(
        &self,
        scope: AiMeteredScope<'_>,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredCredit>, MeteredUsageSourceError>;

    /// The organization's rows per product, SKU and repository, each on its
    /// UTC day.
    async fn org_usage(
        &self,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredOrgCharge>, MeteredUsageSourceError>;

    /// The month's summary per product and SKU: seats and the pool discount.
    async fn summary(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiMeteredSummaryLine>, MeteredUsageSourceError>;
}

/// Held for a whole import; releasing or dropping it frees the import lock.
#[async_trait]
pub trait AiImportLease: Send {
    async fn release(self: Box<Self>) -> Result<(), AiMeteredUsageError>;
}

/// The imported usage of metered payers, its coverage and its runs.
#[async_trait]
pub trait AiMeteredUsageRepository: Send + Sync + 'static {
    /// The store's clock.
    async fn now(&self) -> Result<OffsetDateTime, AiMeteredUsageError>;

    /// Takes the payer's import lock for as long as the lease is held; `None`
    /// when another import holds it.
    async fn try_lock_import(
        &self,
        payer: AiProvider,
    ) -> Result<Option<Box<dyn AiImportLease>>, AiMeteredUsageError>;

    /// The stored slices of `source` on days within `dates` (a summary's by
    /// its month's first day).
    async fn slices(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiSliceRecord>, AiMeteredUsageError>;

    /// The stored gross quantity per key of an account or the organization
    /// over `month`.
    async fn credit_month_totals(
        &self,
        payer: AiProvider,
        scope: AiStoredScope,
        month: AiBillingMonth,
    ) -> Result<Vec<AiCreditTotal>, AiMeteredUsageError>;

    /// Replaces each given day of `scope` in one transaction: a day's rows are
    /// rewritten only where they differ (keys and every value), and every
    /// day's slice is recorded as fetched at `fetched_at`. An empty day records
    /// that the day holds nothing. Only a change in what the payer sent (keys,
    /// units, unit prices, amounts) changes a slice; Toki's own fields (the
    /// normalized model, the kind, the requesting login) are rewritten without
    /// one. Returns how many days the payer changed.
    async fn replace_credit_days(
        &self,
        payer: AiProvider,
        scope: AiCreditStoreScope<'_>,
        days: &[(Date, Vec<AiMeteredCredit>)],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError>;

    /// Replaces the organization's usage rows of every day of `days` from one
    /// fetch, in one transaction, by the same rule. Returns how many days the
    /// payer changed.
    async fn replace_org_usage(
        &self,
        payer: AiProvider,
        days: AiUsageDateRange,
        rows: &[AiMeteredOrgCharge],
        fetched_at: OffsetDateTime,
    ) -> Result<u32, AiMeteredUsageError>;

    /// Replaces a month's summary by the same rule. Returns whether the payer
    /// changed it.
    async fn replace_summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        rows: &[AiMeteredSummaryLine],
        fetched_at: OffsetDateTime,
    ) -> Result<bool, AiMeteredUsageError>;

    /// Marks every stored check of `month` as being imported by `run`, before
    /// the run touches any of the month's days. Each scope's next check
    /// clears its mark; one a run left marked is interrupted (`checks`).
    async fn mark_checks_importing(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        run: AiImportRunId,
    ) -> Result<(), AiMeteredUsageError>;

    /// Stores the latest check of an account's (or, without one, the
    /// organization's) month against the payer's month total, clearing its
    /// import mark.
    async fn record_check(
        &self,
        payer: AiProvider,
        source: AiMeteredSource,
        month: AiBillingMonth,
        check: &AiUsageCheck,
    ) -> Result<(), AiMeteredUsageError>;

    /// Records a run started at `started_at` as running. The caller holds the
    /// import lock, which proves that every other `running` run is dead: those
    /// are marked failed ("interrupted") first.
    async fn start_run(
        &self,
        payer: AiProvider,
        plan: &AiImportPlan,
        started_at: OffsetDateTime,
    ) -> Result<AiImportRunId, AiMeteredUsageError>;

    /// Stores a finished run and its per-source outcomes.
    async fn finish_run(
        &self,
        payer: AiProvider,
        run: &AiImportRun,
    ) -> Result<(), AiMeteredUsageError>;

    /// The runs' history, the run running now (a `running` run while the
    /// import lock is held), the latest finished run with its account errors,
    /// and each source's coverage.
    async fn import_status(&self, payer: AiProvider)
        -> Result<AiImportStatus, AiMeteredUsageError>;

    /// Months with any imported slice, oldest first.
    async fn months(&self, payer: AiProvider) -> Result<Vec<AiBillingMonth>, AiMeteredUsageError>;

    /// Stored account credits on days within `dates`, of `accounts` or of
    /// every account.
    async fn account_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        accounts: Option<&[AiProviderAccountId]>,
    ) -> Result<Vec<AiStoredAccountCredit>, AiMeteredUsageError>;

    async fn org_credits(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiStoredOrgCredit>, AiMeteredUsageError>;

    async fn org_charges(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredOrgCharge>, AiMeteredUsageError>;

    /// A month's summary with when its content was fetched.
    async fn summary(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
    ) -> Result<Option<(Vec<AiMeteredSummaryLine>, OffsetDateTime)>, AiMeteredUsageError>;

    /// The latest checks of `month`, the organization's included. A check
    /// still marked by a run other than `importing`, the run importing now if
    /// any, is interrupted (`AiUsageCheck::interrupted`): that run stopped or
    /// died before finishing the scope's month. Marks of `importing` keep the
    /// check they marked while it runs. Without `importing`, every mark
    /// counts, as it should while the caller holds the import lock.
    async fn checks(
        &self,
        payer: AiProvider,
        month: AiBillingMonth,
        importing: Option<AiImportRunId>,
    ) -> Result<Vec<AiUsageCheck>, AiMeteredUsageError>;

    /// Usage `payer` paid for (`COALESCE(billing_provider, provider)`) per
    /// user, UTC day, model and destination within `dates`, of `user` or
    /// everyone, with the cost of buckets at or after `reprice_floor` on an
    /// old pricing basis (none, or another than their machine's current one:
    /// the latest non-null basis of its uploads and repricings, in the order
    /// they were applied) and of buckets before it. Keys resolve as for the
    /// bill (`AiMappingResolver` with `mapping_company` as the configured
    /// company): a project, Unassigned unmapped (key names are never
    /// returned) or unattributed; keys their user marked personal are
    /// included, as `Personal`. The costs of a user's day and model are
    /// rounded as one: each destination gets the rounded running total
    /// through it less the one before it, so the destinations sum exactly to
    /// the model's rounded cost.
    async fn tracked_usage(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
        reprice_floor: OffsetDateTime,
        mapping_company: Option<&TimeTrackingCompany>,
        user: Option<UserId>,
    ) -> Result<Vec<AiTrackedModelDay>, AiMeteredUsageError>;
}
