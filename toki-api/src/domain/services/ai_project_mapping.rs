use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    models::{
        AiMappedProject, AiProjectKey, AiProjectMapping, AiProjectMappingActor,
        AiProjectMappingEntry, AiUsageScope, AiUsageTimeZone, Project, ProjectId,
        TimeTrackingCompany, UnmappedAiProjectKey,
    },
    ports::{
        inbound::AiProjectMappingService,
        outbound::{AiProjectMappingRepository, TimeTrackingProjectCatalog},
    },
    AiProjectMappingError, AuthMethod,
};

pub struct AiProjectMappingServiceImpl<R, P> {
    repository: Arc<R>,
    projects: Option<Arc<P>>,
    time_zone: AiUsageTimeZone,
}

impl<R, P> AiProjectMappingServiceImpl<R, P> {
    /// `projects` is the configured time-tracking company's project list, or
    /// `None` when time tracking is not configured: then nothing can be mapped
    /// and every mapping is stale. `time_zone` defines the reported local dates.
    pub fn new(repository: Arc<R>, projects: Option<Arc<P>>, time_zone: AiUsageTimeZone) -> Self {
        Self {
            repository,
            projects,
            time_zone,
        }
    }
}

/// Whether the actor manages every mapping. That takes an admin in a browser
/// session: a long-lived API token, such as a sync token on a laptop, must not
/// re-attribute the whole team's usage.
fn manages_every_mapping(actor: &AiProjectMappingActor) -> bool {
    actor.is_admin && actor.authenticated_by == AuthMethod::Session
}

fn scope(actor: &AiProjectMappingActor) -> AiUsageScope {
    if manages_every_mapping(actor) {
        AiUsageScope::Everyone
    } else {
        AiUsageScope::User(actor.user_id)
    }
}

impl<R, P: TimeTrackingProjectCatalog> AiProjectMappingServiceImpl<R, P> {
    fn catalog(&self) -> Result<&P, AiProjectMappingError> {
        self.projects
            .as_deref()
            .ok_or(AiProjectMappingError::NotConfigured)
    }

    fn configured_company(&self) -> Option<&TimeTrackingCompany> {
        self.projects.as_deref().map(|projects| projects.company())
    }

    fn entry(&self, mapping: AiProjectMapping) -> AiProjectMappingEntry {
        AiProjectMappingEntry {
            stale: !mapping.resolves_for(self.configured_company()),
            mapping,
        }
    }
}

#[async_trait]
impl<R, P> AiProjectMappingService for AiProjectMappingServiceImpl<R, P>
where
    R: AiProjectMappingRepository,
    P: TimeTrackingProjectCatalog,
{
    async fn list_mappings(
        &self,
        actor: &AiProjectMappingActor,
    ) -> Result<Vec<AiProjectMappingEntry>, AiProjectMappingError> {
        let mappings = self.repository.list(scope(actor)).await?;

        Ok(mappings
            .into_iter()
            .map(|mapping| self.entry(mapping))
            .collect())
    }

    async fn list_unmapped_keys(
        &self,
        actor: &AiProjectMappingActor,
    ) -> Result<Vec<UnmappedAiProjectKey>, AiProjectMappingError> {
        self.repository
            .list_unmapped(scope(actor), &self.time_zone)
            .await
    }

    async fn list_projects(&self) -> Result<Vec<Project>, AiProjectMappingError> {
        self.catalog()?
            .active_projects()
            .await
            .map_err(|error| AiProjectMappingError::ProjectsUnavailable(error.to_string()))
    }

    async fn set_mapping(
        &self,
        actor: &AiProjectMappingActor,
        project_key: &str,
        project_id: &ProjectId,
    ) -> Result<AiProjectMappingEntry, AiProjectMappingError> {
        let project_key = AiProjectKey::parse(project_key)?;
        let company = self.catalog()?.company();
        let admin = manages_every_mapping(actor);
        if !admin
            && !self
                .repository
                .appears_in_usage(&actor.user_id, &project_key)
                .await?
        {
            return Err(AiProjectMappingError::NotInOwnUsage);
        }

        let project = self
            .list_projects()
            .await?
            .into_iter()
            .find(|project| &project.id == project_id)
            .ok_or_else(|| AiProjectMappingError::UnknownProject(project_id.to_string()))?;
        let project = AiMappedProject {
            id: project.id,
            name: project.name,
        };

        let mapping = if admin {
            self.repository
                .upsert(&project_key, company, &project, &actor.user_id)
                .await?
        } else {
            self.repository
                .create(&project_key, company, &project, &actor.user_id)
                .await?
                .ok_or(AiProjectMappingError::AlreadyMapped)?
        };

        Ok(self.entry(mapping))
    }

    async fn delete_mapping(
        &self,
        actor: &AiProjectMappingActor,
        project_key: &str,
    ) -> Result<(), AiProjectMappingError> {
        let project_key = AiProjectKey::parse(project_key)?;
        if !manages_every_mapping(actor) {
            return Err(AiProjectMappingError::AdminOnly);
        }

        if self.repository.delete(&project_key).await? {
            Ok(())
        } else {
            Err(AiProjectMappingError::NotFound)
        }
    }
}
