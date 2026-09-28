use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        AiMachineId, AiMachineStatus, AiSessionCursor, AiSessionOrder, AiStoredMapping,
        AiUsageDateRange, AiUsageOptionSums, AiUsageSelection, AiUsageSessionRows, AiUsageSums,
        AiUsageTimeZone, AiUsageTotals, UserId,
    },
    AiUsageError,
};

/// Reads of one user's own AI usage. Every method covers `user_id`'s usage and
/// machines only. Local dates and hours are in `time_zone`: a bucket belongs to
/// the local date and hour its start instant falls in. Costs are summed exactly
/// and each sum converted once, so sums agree with every other report. The store
/// never decides attribution: selections name the keys to keep and the keys of
/// each project.
#[async_trait]
pub trait AiUsageReportRepository: Send + Sync + 'static {
    /// Today's date in `time_zone`, by the store's clock.
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiUsageError>;

    /// Every stored project mapping, team-wide.
    async fn stored_mappings(&self) -> Result<Vec<AiStoredMapping>, AiUsageError>;

    /// The selected usage with local dates in `dates`, summed in total and per
    /// local day, provider and model; project; project key; provider and model;
    /// machine; and local weekday and hour.
    async fn usage_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        selection: &AiUsageSelection,
    ) -> Result<AiUsageSums, AiUsageError>;

    /// The providers, models (with their sums, for ordering), project keys and
    /// machines with usage in `dates`, whatever a selection would keep.
    async fn option_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<AiUsageOptionSums, AiUsageError>;

    /// The selected usage in `dates` summed per session, largest first in
    /// `order`: at most `limit` sessions after `after`, and how many there are
    /// in all.
    #[allow(clippy::too_many_arguments)]
    async fn sessions(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        selection: &AiUsageSelection,
        order: AiSessionOrder,
        after: Option<&AiSessionCursor>,
        limit: usize,
    ) -> Result<AiUsageSessionRows, AiUsageError>;

    /// The user's machines with the coverage each provider last reported, most
    /// recently synced first.
    async fn machines(&self, user_id: &UserId) -> Result<Vec<AiMachineStatus>, AiUsageError>;

    /// All usage in `dates` summed per machine, whatever a report filters.
    async fn machine_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<(AiMachineId, AiUsageTotals)>, AiUsageError>;

    /// Every project key in the user's usage, most recently used first, with
    /// the latest local date of use.
    async fn project_keys(
        &self,
        user_id: &UserId,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<(String, Date)>, AiUsageError>;
}
