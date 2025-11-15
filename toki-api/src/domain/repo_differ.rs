use core::fmt;
use std::{collections::HashMap, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{Identity, RepoClient};
use serde::Serialize;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use tokio::sync::{mpsc, RwLock};
use tracing::instrument;

use crate::domain::Email;
use crate::services::IdentityProcessor;

use super::{NotificationHandler, PullRequest, PullRequestDiff, RepoKey};

#[derive(Debug, thiserror::Error)]
pub enum RepoDifferError {
    #[error("Could not fetch pull requests for repo")]
    PullRequests,
    #[error("Could not fetch threads for pull request")]
    Threads,
    #[error("Could not fetch commits for pull request")]
    Commits,
    #[error("Could not fetch work items for pull request")]
    WorkItems,
    #[error("Could not fetch identities")]
    Identities,
}

impl IntoResponse for RepoDifferError {
    fn into_response(self) -> Response {
        let status = StatusCode::INTERNAL_SERVER_ERROR;

        (status, self.to_string()).into_response()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum RepoDifferStatus {
    Running,
    Stopped,
    Errored,
}

#[derive(Debug, Clone)]
pub enum RepoDifferMessage {
    Start(Duration),
    ForceUpdate,
    Stop,
}

#[derive(Clone)]
pub struct RepoDiffer {
    pub key: RepoKey,
    az_client: RepoClient,
    notification_handler: Arc<NotificationHandler>,
    identity_processor: Arc<IdentityProcessor>,
    pub identities: Arc<RwLock<CachedIdentities>>,
    pub prev_pull_requests: Arc<RwLock<Option<Vec<PullRequest>>>>,
    pub status: Arc<RwLock<RepoDifferStatus>>,
    pub last_updated: Arc<RwLock<Option<OffsetDateTime>>>,
    pub interval: Arc<RwLock<Option<Duration>>>,
}

impl RepoDiffer {
    pub fn new(
        key: RepoKey,
        az_client: RepoClient,
        notification_handler: Arc<NotificationHandler>,
        identity_processor: Arc<IdentityProcessor>,
    ) -> Self {
        Self {
            key,
            az_client,
            notification_handler,
            identity_processor,
            identities: Arc::new(RwLock::new(CachedIdentities::new(Duration::hours(1)))),
            prev_pull_requests: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(RepoDifferStatus::Stopped)),
            last_updated: Arc::new(RwLock::new(None)),
            interval: Arc::new(RwLock::new(None)),
        }
    }

    async fn is_running(&self) -> bool {
        *self.status.read().await == RepoDifferStatus::Running
    }
}

impl RepoDiffer {
    const MAX_RETRIES: usize = 10;
    const INITIAL_RETRY_DELAY: Duration = Duration::seconds(30);
    const MAX_RETRY_DELAY: Duration = Duration::hours(1);

    #[instrument(name = "RepoDiffer::run", skip(self, receiver), fields(key = %self.key))]
    pub async fn run(&self, mut receiver: mpsc::Receiver<RepoDifferMessage>, db_pool: Arc<PgPool>) {
        let mut tick_interval: Option<tokio::time::Interval> = None;

        loop {
            tokio::select! {
                Some(message) = receiver.recv() => {
                    match message {
                        RepoDifferMessage::Start(duration) => {
                            tracing::debug!(
                                "Starting differ {} with interval: {:?}",
                                self.key,
                                duration
                            );
                            tick_interval = Some(tokio::time::interval(std::time::Duration::try_from(duration).unwrap_or(std::time::Duration::from_secs(300))));
                            self.interval.write().await.replace(duration);
                            *self.status.write().await = RepoDifferStatus::Running;
                        }
                        RepoDifferMessage::ForceUpdate => {
                            // TODO: timeout
                            tracing::debug!("Forcing update for differ {}", self.key);
                            let _ = self.tick().await;
                        }
                        RepoDifferMessage::Stop => {
                            tracing::debug!("Stopping differ {}", self.key);
                            tick_interval = None;
                            self.interval.write().await.take();
                            *self.status.write().await = RepoDifferStatus::Stopped;
                        }
                    }
                }
                _ = interval_tick_or_sleep(&mut tick_interval) => {
                    tracing::debug!("Ticked");
                    let mut retries = 0;
                    let mut last_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;

                    'retry_loop: while retries < Self::MAX_RETRIES && self.is_running().await {
                        match tokio::time::timeout(std::time::Duration::from_secs(120), self.tick()).await {
                            Ok(Ok(change_events)) => {
                                if !change_events.is_empty() {
                                    if let Err(e) = self.notification_handler.notify_affected_users(change_events).await {
                                        tracing::error!("Failed to notify affected users: {}", e);
                                    }
                                } else {
                                    tracing::debug!("No changes to notify for {}", self.key);
                                }
                                break 'retry_loop;
                            }
                            Ok(Err(err)) => {
                                tracing::error!("Error ticking for {}: {:?}", self.key, err);
                                last_error = Some(Box::new(err));
                            }
                            Err(_) => {
                                tracing::error!("Tick operation timed out for {}", self.key);
                                last_error = Some(Box::new(std::io::Error::new(std::io::ErrorKind::TimedOut, "Tick operation timed out")));
                            }
                        }

                        retries += 1;
                        if retries < Self::MAX_RETRIES {
                            let backoff_duration = Self::calculate_backoff_duration(retries);
                            tracing::warn!(
                                "Retrying tick operation for {} (attempt {}/{}) after {:?}",
                                self.key,
                                retries + 1,
                                Self::MAX_RETRIES,
                                backoff_duration
                            );
                            tokio::time::sleep(backoff_duration).await;
                        }
                    }

                    if retries == Self::MAX_RETRIES {
                        tracing::error!("All retry attempts failed for {}. Last error: {:?}", self.key, last_error);
                        *self.status.write().await = RepoDifferStatus::Errored;
                    }
                }
            }
        }
    }

    fn calculate_backoff_duration(retry_count: usize) -> std::time::Duration {
        let base = Self::INITIAL_RETRY_DELAY.as_seconds_f64();
        let max = Self::MAX_RETRY_DELAY.as_seconds_f64();

        // initial_delay * 2^retry_count
        let exp_backoff = base * (2_f64.powi(retry_count as i32));
        let final_delay = exp_backoff.min(max);

        std::time::Duration::from_secs_f64(final_delay)
    }

    #[instrument(name = "RepoDiffer::tick", skip(self), fields(key = %self.key))]
    async fn tick(&self) -> Result<Vec<PullRequestDiff>, RepoDifferError> {
        let base_pull_requests = self
            .az_client
            .get_open_pull_requests()
            .await
            .map_err(|_| RepoDifferError::PullRequests)?;

        let mut complete_pull_requests = Vec::new();
        for pr in base_pull_requests {
            let commits = pr
                .commits(&self.az_client)
                .await
                .map_err(|_| RepoDifferError::Commits)?;
            let work_items = pr
                .work_items(&self.az_client)
                .await
                .map_err(|_| RepoDifferError::WorkItems)?;
            let threads = pr
                .threads(&self.az_client)
                .await
                .map_err(|_| RepoDifferError::Threads)?;

            complete_pull_requests.push(PullRequest::new(
                &self.key, pr, threads, commits, work_items,
            ));
        }

        let id_to_email_map = {
            let cached_identities = self.identities.read().await;
            // Update the cached identities if they are stale.
            if cached_identities.is_stale() {
                let identities = self
                    .az_client
                    .get_git_identities()
                    .await
                    .map_err(|_| RepoDifferError::Identities)?;

                // Process identities to cache avatars
                let processed_identities =
                    self.identity_processor.process_identities(identities).await;

                drop(cached_identities); // Drop the read lock before acquiring write lock to avoid deadlock
                let mut cached_identities = self.identities.write().await;
                cached_identities.update(processed_identities);
                cached_identities.id_to_email_map()
            } else {
                cached_identities.id_to_email_map()
            }
        };

        let change_events = {
            let prev_pull_requests = self.prev_pull_requests.read().await;
            match prev_pull_requests.clone() {
                Some(prev_pull_requests) => prev_pull_requests
                    .iter()
                    .map(|prev_pr| {
                        prev_pr.changelog(
                            complete_pull_requests
                                .iter()
                                .find(|p| p.pull_request_base.id == prev_pr.pull_request_base.id),
                            &id_to_email_map,
                        )
                    })
                    .filter(|diff| !diff.changes.is_empty())
                    .collect::<Vec<PullRequestDiff>>(),
                None => Vec::new(),
            }
        };

        self.prev_pull_requests
            .write()
            .await
            .replace(complete_pull_requests);
        self.last_updated
            .write()
            .await
            .replace(OffsetDateTime::now_utc());

        Ok(change_events)
    }
}

impl fmt::Debug for RepoDiffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoDiffer")
            .field("key", &self.key)
            .field("prev_pull_requests", &self.prev_pull_requests)
            .field("last_updated", &self.last_updated)
            .finish()
    }
}

async fn interval_tick_or_sleep(interval: &mut Option<tokio::time::Interval>) {
    if let Some(interval) = interval {
        interval.tick().await;
    } else {
        // Sleep for a very long time to mimic a pending future.
        tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
    }
}

#[derive(Debug, Clone, Default)]
pub struct CachedIdentities {
    pub identities: Vec<Identity>,
    last_updated: Option<OffsetDateTime>,
    stale_after: Duration,
}

impl CachedIdentities {
    pub fn new(stale_after: Duration) -> Self {
        Self {
            identities: Vec::new(),
            last_updated: None,
            stale_after,
        }
    }

    pub fn is_stale(&self) -> bool {
        self.last_updated.is_none_or(|last_updated| {
            (OffsetDateTime::now_utc() - last_updated).unsigned_abs() > self.stale_after
        })
    }

    /// Update the cached identities and set the last updated time to now.
    pub fn update(&mut self, identities: Vec<Identity>) {
        self.identities = identities;
        self.last_updated = Some(OffsetDateTime::now_utc());
    }

    pub fn id_to_name_map(&self) -> HashMap<String, String> {
        self.identities
            .iter()
            .map(|i| (i.id.to_uppercase(), i.display_name.clone()))
            .collect::<HashMap<_, _>>()
    }

    pub fn id_to_email_map(&self) -> HashMap<String, Email> {
        self.identities
            .iter()
            .filter_map(|i| {
                Email::try_from(i.unique_name.as_str())
                    .map(|email| (i.id.to_uppercase(), email))
                    .ok()
            })
            .collect::<HashMap<_, _>>()
    }
}
