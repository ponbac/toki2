use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AiBillingMonth, AiImportFreshness, AiImportPlan, AiImportRun, AiImportRunId,
        AiMeteredMonth, AiMonthMode, UserId,
    },
    ports::outbound::AiImportLease,
    AiMeteredUsageError,
};

/// A begun import: recorded as running, holding the in-process and the store's
/// import locks until it runs to the end.
pub struct AiImportTicket {
    pub run_id: AiImportRunId,
    pub started_at: OffsetDateTime,
    pub plan: AiImportPlan,
    pub lease: Box<dyn AiImportLease>,
    pub guard: tokio::sync::OwnedMutexGuard<()>,
}

/// A metered payer's billed usage: its import and its reconciliation with
/// Toki's tracked usage. Callers hold admin power.
#[async_trait]
pub trait AiMeteredUsageService: Send + Sync + 'static {
    /// The month's reconciliation of every account, with the import's state.
    async fn reconciliation(
        &self,
        month: AiBillingMonth,
    ) -> Result<AiMeteredMonth, AiMeteredUsageError>;

    /// The month restricted to one user's linked days, without the residual,
    /// repositories or pool. `UserNotFound` for an unknown user.
    async fn developer_reconciliation(
        &self,
        month: AiBillingMonth,
        user: UserId,
    ) -> Result<AiMeteredMonth, AiMeteredUsageError>;

    /// Takes the locks and records a running run of `month` in `mode`, plus
    /// months with gaps. `ImportRunning` when a lock is taken (by another
    /// import, or a month close holding imports off), `SourceOff` or
    /// `SourceMisconfigured` without a usable source, and `Invalid` for a
    /// month outside the imported range.
    async fn begin_import(
        &self,
        month: AiBillingMonth,
        mode: AiMonthMode,
        by: UserId,
    ) -> Result<AiImportTicket, AiMeteredUsageError>;

    /// Runs a begun import to the end and releases its locks, also on error.
    async fn run_import(&self, ticket: AiImportTicket) -> Result<AiImportRun, AiMeteredUsageError>;

    /// For the scheduler: begins and runs the plan due now. `None` when
    /// nothing is due or a lock is taken (another import, or a month close
    /// holding imports off), which leaves the plan to the next tick.
    async fn import_if_due(&self) -> Result<Option<AiImportRun>, AiMeteredUsageError>;

    async fn freshness(&self) -> Result<AiImportFreshness, AiMeteredUsageError>;
}
