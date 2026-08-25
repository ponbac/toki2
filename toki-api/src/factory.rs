//! Composition root — concrete factories for creating service instances.
//!
//! This is the ONLY place that imports concrete outbound adapters and provider types.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use az_devops::RepoClient;
use kleer::KleerCredentials;
use tokio::sync::RwLock;
use url::Url;

use crate::{
    adapters::{
        inbound::http::{
            TimeTrackingServiceError, TimeTrackingServiceFactory, WorkItemServiceError,
            WorkItemServiceFactory,
        },
        outbound::{
            azure_devops::{AzureDevOpsWorkItemAdapter, AzureDevOpsWorkItemMetadataCache},
            kleer::{KleerAdapter, KleerMetadataCache},
            postgres::PostgresTimerHistoryAdapter,
        },
    },
    config::KleerSettings,
    domain::{
        models::{
            TimeTrackingConnection, TimeTrackingUserLink, UserId, WorkItemProject,
            KLEER_TIME_TRACKING_PROVIDER,
        },
        ports::{
            inbound::{TimeTrackingService, WorkItemService},
            outbound::TimeTrackingUserLinkRepository,
        },
        services::{TimeTrackingServiceImpl, WorkItemServiceImpl},
        RepoKey,
    },
    repositories::{TimerRepositoryImpl, UserRepository, UserRepositoryImpl},
};

/// Concrete factory that creates Kleer-backed TimeTrackingService instances.
pub struct KleerServiceFactory {
    timer_repo: Arc<TimerRepositoryImpl>,
    user_link_repo: Arc<dyn TimeTrackingUserLinkRepository>,
    credentials: Result<KleerCredentials, String>,
    metadata_cache: Arc<KleerMetadataCache>,
}

impl KleerServiceFactory {
    pub fn new(
        timer_repo: Arc<TimerRepositoryImpl>,
        user_link_repo: Arc<dyn TimeTrackingUserLinkRepository>,
        settings: KleerSettings,
    ) -> Self {
        Self {
            timer_repo,
            user_link_repo,
            credentials: settings.credentials(),
            metadata_cache: Arc::new(KleerMetadataCache::new()),
        }
    }

    fn credentials(&self) -> Result<KleerCredentials, TimeTrackingServiceError> {
        self.credentials
            .clone()
            .map_err(TimeTrackingServiceError::configuration)
    }

    async fn resolve_connection(
        &self,
        user_id: UserId,
        provider_company_id: &str,
    ) -> Result<TimeTrackingConnection, TimeTrackingServiceError> {
        let link = self
            .user_link_repo
            .get_active_link_for_user(&user_id, KLEER_TIME_TRACKING_PROVIDER)
            .await
            .map_err(|error| TimeTrackingServiceError::storage(error.to_string()))?;

        Ok(Self::connection_from_link(link, provider_company_id))
    }

    fn connection_from_link(
        link: Option<TimeTrackingUserLink>,
        provider_company_id: &str,
    ) -> TimeTrackingConnection {
        match link.filter(|link| link.provider_company_id == provider_company_id) {
            Some(TimeTrackingUserLink {
                provider,
                provider_user_id,
                provider_user_email,
                provider_user_name,
                ..
            }) => TimeTrackingConnection::Connected {
                provider,
                provider_user_id,
                provider_user_email,
                provider_user_name,
            },
            None => TimeTrackingConnection::Disconnected {
                provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
            },
        }
    }
}

#[async_trait]
impl TimeTrackingServiceFactory for KleerServiceFactory {
    async fn connection_status(
        &self,
        user_id: UserId,
    ) -> Result<TimeTrackingConnection, TimeTrackingServiceError> {
        let credentials = self.credentials()?;
        self.resolve_connection(user_id, &credentials.company_id)
            .await
    }

    async fn create_service(
        &self,
        user_id: UserId,
    ) -> Result<Box<dyn TimeTrackingService>, TimeTrackingServiceError> {
        let credentials = self.credentials()?;
        let connection = self
            .resolve_connection(user_id, &credentials.company_id)
            .await?;
        let TimeTrackingConnection::Connected {
            provider_user_id, ..
        } = connection
        else {
            return Err(TimeTrackingServiceError::not_connected(
                "Your Toki account is not connected to a Kleer user. Contact an admin to set up time tracking access.",
            ));
        };
        let kleer_user_id = provider_user_id.parse::<i64>().map_err(|_| {
            TimeTrackingServiceError::internal(format!(
                "invalid Kleer user id in mapping for Toki user {user_id}"
            ))
        })?;
        let adapter = KleerAdapter::with_metadata_cache(
            credentials,
            kleer_user_id,
            self.metadata_cache.clone(),
        )
        .map_err(|error| {
            TimeTrackingServiceError::configuration(format!(
                "failed to create Kleer service: {error}"
            ))
        })?;
        let history_adapter = PostgresTimerHistoryAdapter::new(self.timer_repo.clone());
        let service = TimeTrackingServiceImpl::new(Arc::new(adapter), Arc::new(history_adapter));

        Ok(Box::new(service))
    }
}

// ---------------------------------------------------------------------------
// Work Items factory
// ---------------------------------------------------------------------------

/// Concrete factory that creates Azure DevOps-backed WorkItemService instances.
///
/// Finds a `RepoClient` matching the requested organization and project,
/// wraps it in an `AzureDevOpsWorkItemAdapter`, and returns a `WorkItemServiceImpl`.
pub struct AzureDevOpsWorkItemServiceFactory {
    repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
    user_repo: Arc<UserRepositoryImpl>,
    api_base_url: Url,
    metadata_cache: Arc<AzureDevOpsWorkItemMetadataCache>,
}

impl AzureDevOpsWorkItemServiceFactory {
    pub fn new(
        repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
        user_repo: Arc<UserRepositoryImpl>,
        api_base_url: Url,
    ) -> Self {
        Self {
            repo_clients,
            user_repo,
            api_base_url,
            metadata_cache: Arc::new(AzureDevOpsWorkItemMetadataCache::new()),
        }
    }
}

#[async_trait]
impl WorkItemServiceFactory for AzureDevOpsWorkItemServiceFactory {
    async fn create_service(
        &self,
        organization: &str,
        project: &str,
    ) -> Result<Box<dyn WorkItemService>, WorkItemServiceError> {
        // 1. Find any RepoClient matching the requested org+project
        let clients = self.repo_clients.read().await;
        let client = clients
            .iter()
            .find(|(key, _)| {
                key.organization.eq_ignore_ascii_case(organization)
                    && key.project.eq_ignore_ascii_case(project)
            })
            .map(|(_, client)| client.clone())
            .ok_or_else(|| WorkItemServiceError {
                status: StatusCode::NOT_FOUND,
                message: format!("No client found for {}/{}", organization, project),
            })?;

        // 2. Create adapter and service
        let adapter = AzureDevOpsWorkItemAdapter::with_metadata_cache(
            client,
            self.api_base_url.clone(),
            self.metadata_cache.clone(),
        );
        let service = WorkItemServiceImpl::new(Arc::new(adapter));
        Ok(Box::new(service))
    }

    async fn get_available_projects(
        &self,
        user_id: UserId,
    ) -> Result<Vec<WorkItemProject>, WorkItemServiceError> {
        // Get followed repositories for this user
        let repos = self
            .user_repo
            .followed_repositories(user_id)
            .await
            .map_err(|e| WorkItemServiceError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("Failed to fetch followed repositories: {}", e),
            })?;

        let clients = self.repo_clients.read().await;
        let available_projects: HashSet<(String, String)> = clients
            .keys()
            .map(|key| {
                (
                    key.organization.to_ascii_lowercase(),
                    key.project.to_ascii_lowercase(),
                )
            })
            .collect();

        // Deduplicate into unique (organization, project) pairs that have a live client.
        let mut seen = HashSet::new();
        let projects = repos
            .into_iter()
            .filter(|repo| {
                available_projects.contains(&(
                    repo.organization.to_ascii_lowercase(),
                    repo.project.to_ascii_lowercase(),
                ))
            })
            .filter(|repo| seen.insert((repo.organization.clone(), repo.project.clone())))
            .map(|repo| WorkItemProject {
                organization: repo.organization,
                project: repo.project,
            })
            .collect();

        Ok(projects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn maps_matching_provider_connection() {
        let connection =
            KleerServiceFactory::connection_from_link(Some(test_link("company-1")), "company-1");

        assert_eq!(
            connection,
            TimeTrackingConnection::Connected {
                provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
                provider_user_id: "42".to_string(),
                provider_user_email: Some("ada@example.com".to_string()),
                provider_user_name: Some("Ada Lovelace".to_string()),
            }
        );
    }

    #[test]
    fn treats_other_provider_company_link_as_disconnected() {
        let connection = KleerServiceFactory::connection_from_link(
            Some(test_link("other-company")),
            "company-1",
        );

        assert_eq!(
            connection,
            TimeTrackingConnection::Disconnected {
                provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
            }
        );
    }

    fn test_link(provider_company_id: &str) -> TimeTrackingUserLink {
        TimeTrackingUserLink {
            id: 1,
            user_id: UserId::new(7),
            provider: KLEER_TIME_TRACKING_PROVIDER.to_string(),
            provider_company_id: provider_company_id.to_string(),
            provider_user_id: "42".to_string(),
            provider_user_email: Some("ada@example.com".to_string()),
            provider_user_name: Some("Ada Lovelace".to_string()),
            active: true,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            last_synced_at: OffsetDateTime::UNIX_EPOCH,
        }
    }
}
