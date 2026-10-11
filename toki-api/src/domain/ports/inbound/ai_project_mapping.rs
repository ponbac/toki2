use async_trait::async_trait;

use crate::domain::{
    models::{
        AiProjectMappingActor, AiProjectMappingEntry, Project, ProjectId, UnmappedAiProjectKey,
    },
    AiProjectMappingError,
};

/// Use cases for mapping AI usage project keys to time-tracking projects.
///
/// Mappings are team-wide, one per key, so changing one re-attributes everyone's
/// usage of the key. Developers may therefore only create the first mapping of a
/// key in their own usage. Changing and deleting mappings, and seeing everyone's,
/// needs an admin in a browser session: an API token never has admin powers.
#[async_trait]
pub trait AiProjectMappingService: Send + Sync + 'static {
    /// The mappings of project keys in the actor's own usage, or every mapping
    /// for an admin session, ordered by project key.
    async fn list_mappings(
        &self,
        actor: &AiProjectMappingActor,
    ) -> Result<Vec<AiProjectMappingEntry>, AiProjectMappingError>;

    /// Project keys with usage but no mapping, in the actor's own usage or
    /// everyone's for an admin session, most recently used first. Unmappable
    /// keys such as `unattributed` are listed and marked as such, and no key
    /// is mappable while time tracking is not configured.
    async fn list_unmapped_keys(
        &self,
        actor: &AiProjectMappingActor,
    ) -> Result<Vec<UnmappedAiProjectKey>, AiProjectMappingError>;

    /// The time-tracking projects a key can be mapped to.
    async fn list_projects(&self) -> Result<Vec<Project>, AiProjectMappingError>;

    /// Maps a project key to an active time-tracking project, recording the
    /// project's current name and the actor.
    async fn set_mapping(
        &self,
        actor: &AiProjectMappingActor,
        project_key: &str,
        project_id: &ProjectId,
    ) -> Result<AiProjectMappingEntry, AiProjectMappingError>;

    /// Removes a project key's mapping, leaving its usage unassigned.
    async fn delete_mapping(
        &self,
        actor: &AiProjectMappingActor,
        project_key: &str,
    ) -> Result<(), AiProjectMappingError>;
}
