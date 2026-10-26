use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AiBilledEntry, AiBillingMonth, AiMonthClose, AiMonthCloseId, AiNewClose, AiStoredSnapshot,
        AiUsageTimeZone, UserId,
    },
    AiBillingError, AiMonthCloseError,
};

/// Held while a month is closed or re-opened; releasing or dropping it frees
/// the lock.
#[async_trait]
pub trait AiCloseLease: Send {
    async fn release(self: Box<Self>) -> Result<(), AiBillingError>;
}

/// Month closes of the AI bill and their snapshots.
#[async_trait]
pub trait AiMonthCloseRepository: Send + Sync + 'static {
    /// The store's clock.
    async fn now(&self) -> Result<OffsetDateTime, AiBillingError>;

    /// When the first open month starts: the earliest month without an
    /// active close at or after the month `configured_floor` falls in, from
    /// the earlier of its UTC and its `time_zone` midnight, so every bucket
    /// either basis of that month bills is at or after it. Repricing stops
    /// before it (`reprice_floor`).
    async fn open_from(
        &self,
        time_zone: &AiUsageTimeZone,
        configured_floor: OffsetDateTime,
    ) -> Result<OffsetDateTime, AiBillingError>;

    /// Takes the close lock, so one close or re-open runs at a time; `None`
    /// while another holds it.
    async fn try_lock(&self) -> Result<Option<Box<dyn AiCloseLease>>, AiBillingError>;

    /// The active closes, oldest month first.
    async fn active(&self) -> Result<Vec<AiMonthClose>, AiBillingError>;

    /// Every close of `month`, re-opened ones included, newest first.
    async fn history(&self, month: AiBillingMonth) -> Result<Vec<AiMonthClose>, AiBillingError>;

    /// An id for a close about to be stored, so its snapshot's CSV can name
    /// it.
    async fn reserve_id(&self) -> Result<AiMonthCloseId, AiBillingError>;

    /// Stores a close and its snapshot in one transaction. `AlreadyClosed`
    /// when the month has an active close; `Busy` when a close whose
    /// adjustments the snapshot holds was re-opened meanwhile.
    async fn store(&self, close: &AiNewClose) -> Result<AiMonthClose, AiMonthCloseError>;

    /// A close's own rows (not its adjustment lines), as the netting reads
    /// them.
    async fn own_entries(
        &self,
        close: AiMonthCloseId,
    ) -> Result<Vec<AiBilledEntry>, AiBillingError>;

    /// The rows adjusting `month` in active closes.
    async fn frozen_adjustments(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiBilledEntry>, AiBillingError>;

    /// The months whose active closes hold adjustments to `month`, oldest
    /// first.
    async fn months_adjusting(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiBillingMonth>, AiBillingError>;

    /// What a closed month serves.
    async fn snapshot(&self, close: AiMonthCloseId) -> Result<AiStoredSnapshot, AiBillingError>;

    /// Marks the active close of `month` re-opened by `by`; `NotClosed`
    /// without one.
    async fn reopen(&self, month: AiBillingMonth, by: &UserId) -> Result<(), AiMonthCloseError>;
}
