//! Mapping a metered payer's repositories to time-tracking projects: the
//! projects its code review credits bill to.

use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{
        billed_spelling, repository_key, repository_overview, utc_day, AiBillingMonth,
        AiMappedProject, AiMappingResolution, AiMappingResolver, AiProvider,
        AiRepositoryMappingEntry, AiRepositoryMappingOverview, AiRepositoryName,
        AiRepositoryOverviewInput, ProjectId, TimeTrackingCompany, UserId,
    },
    ports::{
        inbound::AiRepositoryMappingService,
        outbound::{AiRepositoryMappingRepository, TimeTrackingProjectCatalog},
    },
    AiRepositoryMappingError,
};

/// How a payer's repositories are mapped and shown.
#[derive(Debug, Clone)]
pub struct AiRepositoryMappingSettings {
    /// The host of the payer's repositories in project keys, such as
    /// `github.com`: suggestions match `{key_host}/{owner}/{name}`.
    pub key_host: String,
    /// The metered cutover: the first month whose bill has repository lines.
    pub billed_from: Option<AiBillingMonth>,
}

pub struct AiRepositoryMappingServiceImpl<R, P> {
    repository: Arc<R>,
    projects: Option<Arc<P>>,
    payer: AiProvider,
    settings: AiRepositoryMappingSettings,
}

impl<R, P> AiRepositoryMappingServiceImpl<R, P> {
    /// `projects` is the configured time-tracking company's project list, or
    /// `None` when time tracking is not configured: then nothing can be mapped
    /// and no mapping resolves.
    pub fn new(
        repository: Arc<R>,
        projects: Option<Arc<P>>,
        payer: AiProvider,
        settings: AiRepositoryMappingSettings,
    ) -> Self {
        Self {
            repository,
            projects,
            payer,
            settings,
        }
    }
}

impl<R, P: TimeTrackingProjectCatalog> AiRepositoryMappingServiceImpl<R, P> {
    fn catalog(&self) -> Result<&P, AiRepositoryMappingError> {
        self.projects
            .as_deref()
            .ok_or(AiRepositoryMappingError::NotConfigured)
    }

    fn configured_company(&self) -> Option<&TimeTrackingCompany> {
        self.projects.as_deref().map(|projects| projects.company())
    }
}

#[async_trait]
impl<R, P> AiRepositoryMappingService for AiRepositoryMappingServiceImpl<R, P>
where
    R: AiRepositoryMappingRepository,
    P: TimeTrackingProjectCatalog,
{
    async fn overview(&self) -> Result<AiRepositoryMappingOverview, AiRepositoryMappingError> {
        let today = utc_day(self.repository.now().await?);
        let current = AiBillingMonth::containing(today).ok_or_else(|| {
            AiRepositoryMappingError::Storage(format!("no month contains {today}"))
        })?;
        let months: Vec<AiBillingMonth> = current.previous().into_iter().chain([current]).collect();
        let credits = self.repository.repository_credits(self.payer).await?;
        let mappings = self.repository.list(self.payer).await?;
        let keys = AiMappingResolver::new(
            self.repository.key_mappings().await?,
            self.configured_company().cloned(),
        );
        Ok(repository_overview(AiRepositoryOverviewInput {
            payer: self.payer,
            months,
            billed_from: self.settings.billed_from,
            credits,
            mappings,
            configured: self.configured_company(),
            keys: &keys,
            key_host: &self.settings.key_host,
        }))
    }

    async fn set_mapping(
        &self,
        repository: &str,
        project_id: &ProjectId,
        by: UserId,
    ) -> Result<AiRepositoryMappingEntry, AiRepositoryMappingError> {
        let name = AiRepositoryName::parse(repository)?;
        let catalog = self.catalog()?;
        let key = name.key();
        // The payer's latest spelling; else the existing mapping's, which can
        // always be changed, even once the repository is no longer billed.
        let spelling =
            match billed_spelling(&self.repository.repository_credits(self.payer).await?, &key) {
                Some(spelling) => spelling,
                None => self
                    .repository
                    .list(self.payer)
                    .await?
                    .into_iter()
                    .find(|mapping| repository_key(&mapping.repository) == key)
                    .map(|mapping| mapping.repository)
                    .ok_or_else(|| {
                        AiRepositoryMappingError::UnknownRepository(name.as_str().to_string())
                    })?,
            };

        let project = catalog
            .active_projects()
            .await
            .map_err(|error| AiRepositoryMappingError::ProjectsUnavailable(error.to_string()))?
            .into_iter()
            .find(|project| &project.id == project_id)
            .ok_or_else(|| AiRepositoryMappingError::UnknownProject(project_id.to_string()))?;
        let project = AiMappedProject {
            id: project.id,
            name: project.name,
        };

        let mapping = self
            .repository
            .upsert(self.payer, &spelling, catalog.company(), &project, &by)
            .await?;
        Ok(AiRepositoryMappingEntry {
            resolution: AiMappingResolution::of(&mapping.company, Some(catalog.company())),
            mapping,
        })
    }

    async fn delete_mapping(&self, repository: &str) -> Result<(), AiRepositoryMappingError> {
        let name = AiRepositoryName::parse(repository)?;
        if self.repository.delete(self.payer, &name).await? {
            Ok(())
        } else {
            Err(AiRepositoryMappingError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests;
