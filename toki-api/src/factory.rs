//! Composition root — concrete factories for creating service instances.
//!
//! This is the ONLY place that imports concrete outbound adapters and provider types.

use std::collections::HashMap;
use std::ops::Add;
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use az_devops::RepoClient;
use time::{Duration, OffsetDateTime};
use tokio::sync::RwLock;
use url::Url;

use crate::{
    adapters::{
        inbound::http::{
            TimeTrackingServiceError, TimeTrackingServiceFactory, WorkItemServiceError,
            WorkItemServiceFactory,
        },
        outbound::{
            azure_devops::AzureDevOpsWorkItemAdapter,
            milltime::{MilltimeAdapter, MilltimePassword},
            postgres::PostgresTimerHistoryAdapter,
        },
    },
    domain::{
        models::{UserId, WorkItemProject},
        ports::inbound::{TimeTrackingService, WorkItemService},
        services::{TimeTrackingServiceImpl, WorkItemServiceImpl},
        RepoKey,
    },
    repositories::{TimerRepositoryImpl, UserRepository, UserRepositoryImpl},
};

/// Concrete factory that creates Milltime-backed TimeTrackingService instances.
pub struct MilltimeServiceFactory {
    timer_repo: Arc<TimerRepositoryImpl>,
}

impl MilltimeServiceFactory {
    pub fn new(timer_repo: Arc<TimerRepositoryImpl>) -> Self {
        Self { timer_repo }
    }
}

#[async_trait]
impl TimeTrackingServiceFactory for MilltimeServiceFactory {
    async fn create_service(
        &self,
        jar: CookieJar,
        cookie_domain: &str,
    ) -> Result<(Box<dyn TimeTrackingService>, CookieJar), TimeTrackingServiceError> {
        let (credentials, jar) = extract_credentials(jar, cookie_domain).await?;

        let adapter = MilltimeAdapter::new(credentials);
        let history_adapter = PostgresTimerHistoryAdapter::new(self.timer_repo.clone());
        let service = TimeTrackingServiceImpl::new(Arc::new(adapter), Arc::new(history_adapter));

        Ok((Box::new(service), jar))
    }

    async fn authenticate(
        &self,
        username: &str,
        password: &str,
        cookie_domain: &str,
    ) -> Result<CookieJar, TimeTrackingServiceError> {
        let credentials = milltime::Credentials::new(username, password)
            .await
            .map_err(|_| TimeTrackingServiceError::unauthorized("Invalid credentials"))?;

        let encrypted_password = MilltimePassword::new(password.to_string()).to_encrypted();

        let use_secure = cookie_domain != "localhost";

        let mut jar = CookieJar::new()
            .add(
                Cookie::build(("mt_user", username.to_string()))
                    .domain(cookie_domain.to_string())
                    .path("/")
                    .secure(use_secure)
                    .http_only(true)
                    .expires(OffsetDateTime::now_utc().add(Duration::days(180)))
                    .build(),
            )
            .add(
                Cookie::build(("mt_password", encrypted_password))
                    .domain(cookie_domain.to_string())
                    .path("/")
                    .secure(use_secure)
                    .http_only(true)
                    .expires(OffsetDateTime::now_utc().add(Duration::days(180)))
                    .build(),
            );

        for cookie in credentials.auth_cookies(cookie_domain.to_string()) {
            jar = jar.add(cookie);
        }

        Ok(jar)
    }
}

/// Extract provider credentials from cookies.
///
/// First tries to parse existing credential cookies, then falls back to
/// username/password authentication.
async fn extract_credentials(
    jar: CookieJar,
    cookie_domain: &str,
) -> Result<(milltime::Credentials, CookieJar), TimeTrackingServiceError> {
    // Try to use existing credentials from cookies first
    if let Ok(credentials) = jar.clone().try_into() {
        tracing::debug!("using existing provider credentials");
        return Ok((credentials, jar));
    }

    // Fall back to username/password authentication
    let user_cookie = jar
        .get("mt_user")
        .ok_or_else(|| TimeTrackingServiceError::unauthorized("missing mt_user cookie"))?;

    let pass_cookie = jar
        .get("mt_password")
        .ok_or_else(|| TimeTrackingServiceError::unauthorized("missing mt_password cookie"))?;

    let decrypted_pass = MilltimePassword::from_encrypted(pass_cookie.value().to_string());

    let credentials = milltime::Credentials::new(user_cookie.value(), decrypted_pass.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("failed to create provider credentials: {:?}", e);
            TimeTrackingServiceError::unauthorized(e.to_string())
        })?;

    // Update cookies with the new credentials
    let mut updated_jar = jar;
    for cookie in credentials.auth_cookies(cookie_domain.to_string()) {
        updated_jar = updated_jar.add(cookie);
    }

    tracing::debug!("created new provider credentials");
    Ok((credentials, updated_jar))
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

        // Deduplicate into unique (organization, project) pairs
        let mut seen = std::collections::HashSet::new();
        let projects = repos
            .into_iter()
            .filter(|repo| seen.insert((repo.organization.clone(), repo.project.clone())))
            .map(|repo| WorkItemProject {
                organization: repo.organization,
                project: repo.project,
            })
            .collect();

        Ok(projects)
    }
}
