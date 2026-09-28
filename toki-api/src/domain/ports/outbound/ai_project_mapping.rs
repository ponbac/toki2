use async_trait::async_trait;

use crate::domain::{
    models::{
        AiMappedProject, AiProjectKey, AiProjectMapping, AiUsageScope, AiUsageTimeZone,
        TimeTrackingCompany, UnmappedAiProjectKey, UserId,
    },
    AiProjectMappingError,
};

#[async_trait]
pub trait AiProjectMappingRepository: Send + Sync + 'static {
    /// The mappings of project keys in the scope's usage, or every mapping for
    /// `Everyone`, ordered by project key.
    async fn list(
        &self,
        scope: AiUsageScope,
    ) -> Result<Vec<AiProjectMapping>, AiProjectMappingError>;

    /// Project keys in the scope's usage that have no mapping, including
    /// unmappable ones such as `unattributed`, most recently used first. Each
    /// reports its latest local date of use in `time_zone`.
    async fn list_unmapped(
        &self,
        scope: AiUsageScope,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<UnmappedAiProjectKey>, AiProjectMappingError>;

    /// Whether any of the user's stored usage has the project key.
    async fn appears_in_usage(
        &self,
        user_id: &UserId,
        project_key: &AiProjectKey,
    ) -> Result<bool, AiProjectMappingError>;

    /// Maps a key that has no mapping yet, recording `created_by` as its creator
    /// and editor. Returns `None`, and changes nothing, when the key is mapped.
    async fn create(
        &self,
        project_key: &AiProjectKey,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        created_by: &UserId,
    ) -> Result<Option<AiProjectMapping>, AiProjectMappingError>;

    /// Creates or replaces the key's mapping, recording who last saved it and
    /// when. A replaced mapping keeps its creator.
    async fn upsert(
        &self,
        project_key: &AiProjectKey,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        updated_by: &UserId,
    ) -> Result<AiProjectMapping, AiProjectMappingError>;

    /// Deletes the key's mapping and reports whether one existed.
    async fn delete(&self, project_key: &AiProjectKey) -> Result<bool, AiProjectMappingError>;
}
