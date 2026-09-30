use async_trait::async_trait;

use crate::domain::{
    models::{
        AiMachineList, AiProjectKeyList, AiSessionOrder, AiUsageReport, AiUsageReportQuery,
        AiUsageSessionList, UserId,
    },
    AiUsageError,
};

/// A developer's own AI usage. Every use case reads the given user's usage and
/// nobody else's, whoever the user is, admins included: views of other
/// developers' usage belong to the admin overview and stop at days.
#[async_trait]
pub trait AiUsageReportService: Send + Sync + 'static {
    /// The user's usage in the query's range and filter, broken down by day,
    /// project, model, machine and local hour of the week.
    async fn report(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
    ) -> Result<AiUsageReport, AiUsageError>;

    /// A page of the user's sessions in the query's range and filter, largest
    /// first in `order`, continuing after the encoded cursor `after` if given.
    /// `limit` defaults to 100, at most 1000.
    async fn sessions(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
        order: AiSessionOrder,
        after: Option<&str>,
        limit: Option<usize>,
    ) -> Result<AiUsageSessionList, AiUsageError>;

    /// The user's machines, what they last reported, whether they are stale,
    /// and all their usage in the query's range. The query's filter is ignored.
    async fn machines(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
    ) -> Result<AiMachineList, AiUsageError>;

    /// Every project key in the user's usage and how it is attributed.
    async fn project_keys(&self, user_id: &UserId) -> Result<AiProjectKeyList, AiUsageError>;
}
