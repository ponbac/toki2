use async_trait::async_trait;

use crate::domain::{
    models::{AiRepositoryMappingEntry, AiRepositoryMappingOverview, ProjectId, UserId},
    AiRepositoryMappingError,
};

/// A metered payer's repositories and the time-tracking projects their
/// credits (code review) bill to. Mappings are team-wide, one per repository,
/// and resolve whenever a bill is read. Callers hold admin power.
///
/// Repository names come from the payer's billing, which admins see there;
/// they are not project keys. Suggestions come only from project-key mappings
/// that resolve, never from unmapped keys.
#[async_trait]
pub trait AiRepositoryMappingService: Send + Sync + 'static {
    /// Every repository the payer billed and every mapping, with credits in
    /// the previous and current UTC month and suggestions for those without
    /// a resolving mapping.
    async fn overview(&self) -> Result<AiRepositoryMappingOverview, AiRepositoryMappingError>;

    /// Maps a repository to an active time-tracking project, recording the
    /// project's current name and who saved it. A new mapping needs a
    /// repository the payer billed; an existing one can always be changed.
    async fn set_mapping(
        &self,
        repository: &str,
        project_id: &ProjectId,
        by: UserId,
    ) -> Result<AiRepositoryMappingEntry, AiRepositoryMappingError>;

    /// Removes a repository's mapping, so its credits bill as Unassigned.
    async fn delete_mapping(&self, repository: &str) -> Result<(), AiRepositoryMappingError>;
}
