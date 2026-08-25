use async_trait::async_trait;

use crate::domain::{
    models::{
        BeginTimeEntryWrite, PendingTimeEntryWrite, TimeEntryWriteIntent, TimeEntryWriteResolution,
    },
    TimeTrackingError,
};

/// Durable lifecycle for provider time-entry writes.
#[async_trait]
pub trait TimeEntryWriteStore: Send + Sync + 'static {
    /// Persist the exact provider command before deciding who may submit it.
    async fn begin(
        &self,
        intent: TimeEntryWriteIntent,
    ) -> Result<BeginTimeEntryWrite, TimeTrackingError>;

    /// Atomically complete local history, or cancel a command not sent to the provider.
    async fn resolve(
        &self,
        write: &PendingTimeEntryWrite,
        resolution: TimeEntryWriteResolution<'_>,
    ) -> Result<(), TimeTrackingError>;
}
