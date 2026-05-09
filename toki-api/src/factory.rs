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
            azure_devops::AzureDevOpsWorkItemAdapter, kleer::KleerAdapter,
            postgres::PostgresTimerHistoryAdapter,
        },
    },
    config::KleerSettings,
    domain::{
        models::{UserId, WorkItemProject, KLEER_TIME_TRACKING_PROVIDER},
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
        }
    }

    fn credentials(&self) -> Result<KleerCredentials, TimeTrackingServiceError> {
        self.credentials
            .clone()
            .map_err(TimeTrackingServiceError::configuration)
    }

    async fn mapped_kleer_user_id(
        &self,
        user_id: UserId,
        provider_company_id: &str,
    ) -> Result<i64, TimeTrackingServiceError> {
        let link = self
            .user_link_repo
            .get_active_link_for_user(&user_id, KLEER_TIME_TRACKING_PROVIDER)
            .await
            .map_err(|error| TimeTrackingServiceError::internal(error.to_string()))?
            .filter(|link| link.provider_company_id == provider_company_id)
            .ok_or_else(|| {
                TimeTrackingServiceError::not_connected(
                    "Your Toki account is not connected to a Kleer user. Contact an admin to set up time tracking access.",
                )
            })?;

        link.provider_user_id.parse::<i64>().map_err(|_| {
            TimeTrackingServiceError::internal(format!(
                "invalid Kleer user id in mapping for Toki user {user_id}"
            ))
        })
    }
}

#[async_trait]
impl TimeTrackingServiceFactory for KleerServiceFactory {
    async fn create_service(
        &self,
        user_id: UserId,
    ) -> Result<Box<dyn TimeTrackingService>, TimeTrackingServiceError> {
        let credentials = self.credentials()?;
        let kleer_user_id = self
            .mapped_kleer_user_id(user_id, &credentials.company_id)
            .await?;
        let adapter = KleerAdapter::new(credentials, kleer_user_id).map_err(|error| {
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
        let adapter = AzureDevOpsWorkItemAdapter::new(client, self.api_base_url.clone());
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
