use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AiMappedProject, AiProvider, AiRepositoryDayCredits, AiRepositoryMapping, AiRepositoryName,
        AiStoredMapping, TimeTrackingCompany, UserId,
    },
    AiRepositoryMappingError,
};

/// The stored mappings of a metered payer's repositories, and what the
/// mapping overview reads besides them.
#[async_trait]
pub trait AiRepositoryMappingRepository: Send + Sync + 'static {
    /// The store's clock.
    async fn now(&self) -> Result<OffsetDateTime, AiRepositoryMappingError>;

    /// Every mapping of the payer with its editors' names, by repository,
    /// case-insensitively.
    async fn list(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryMapping>, AiRepositoryMappingError>;

    /// Every repository the payer billed credits to (its repository credit
    /// rows), summed per spelling and UTC day.
    async fn repository_credits(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiRepositoryDayCredits>, AiRepositoryMappingError>;

    /// Every stored project-key mapping, for suggestions only. Usage keys are
    /// never read: an unmapped key may name a private repository.
    async fn key_mappings(&self) -> Result<Vec<AiStoredMapping>, AiRepositoryMappingError>;

    /// Creates or replaces the payer's mapping of the repository, matched
    /// case-insensitively, storing `repository` as its name and recording who
    /// last saved it and when. A replaced mapping keeps its creator.
    async fn upsert(
        &self,
        payer: AiProvider,
        repository: &str,
        company: &TimeTrackingCompany,
        project: &AiMappedProject,
        by: &UserId,
    ) -> Result<AiRepositoryMapping, AiRepositoryMappingError>;

    /// Deletes the payer's mapping of the repository, matched
    /// case-insensitively, and reports whether one existed.
    async fn delete(
        &self,
        payer: AiProvider,
        repository: &AiRepositoryName,
    ) -> Result<bool, AiRepositoryMappingError>;
}
