use core::fmt;
use std::{sync::Arc, time::Duration};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{PullRequest, RepoClient};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::{mpsc, RwLock};
use tracing::instrument;

use super::RepoKey;

#[derive(Debug, thiserror::Error)]
pub enum RepoDifferError {
    #[error("Could not fetch pull requests for repo '{0}'")]
    CouldNotFetchPullRequests(RepoKey),
}

impl IntoResponse for RepoDifferError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::CouldNotFetchPullRequests(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum RepoDifferStatus {
    Running,
    Stopped,
    // todo: errored
}

pub enum RepoDifferMessage {
    Start(Duration),
    ForceUpdate,
    Stop,
}

#[derive(Clone)]
pub struct RepoDiffer {
    pub key: RepoKey,
    az_client: RepoClient,
    pub prev_pull_requests: Arc<RwLock<Option<Vec<PullRequest>>>>,
    pub status: Arc<RwLock<RepoDifferStatus>>,
    pub last_updated: Arc<RwLock<Option<OffsetDateTime>>>,
    pub interval: Arc<RwLock<Option<Duration>>>,
}

impl RepoDiffer {
    pub fn new(key: RepoKey, az_client: RepoClient) -> Self {
        Self {
            key,
            az_client,
            prev_pull_requests: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(RepoDifferStatus::Stopped)),
            last_updated: Arc::new(RwLock::new(None)),
            interval: Arc::new(RwLock::new(None)),
        }
    }

    async fn is_stopped(&self) -> bool {
        *self.status.read().await == RepoDifferStatus::Stopped
    }
}

impl RepoDiffer {
    #[instrument(name = "RepoDiffer::run", skip(self, receiver), fields(key = %self.key))]
    pub async fn run(&self, mut receiver: mpsc::Receiver<RepoDifferMessage>) {
        let mut interval: Option<tokio::time::Interval> = None;

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
                            interval = Some(tokio::time::interval(duration));
                            self.interval.write().await.replace(duration);

                            if self.is_stopped().await {
                                *self.status.write().await = RepoDifferStatus::Running;
                            }
                        }
                        RepoDifferMessage::ForceUpdate => {
                            tracing::debug!("Forcing update for differ {}", self.key);
                            let _ = self.tick().await;
                        }
                        RepoDifferMessage::Stop => {
                            tracing::debug!("Stopping differ {}", self.key);
                            interval = None;
                            self.interval.write().await.take();

                            if !self.is_stopped().await {
                                *self.status.write().await = RepoDifferStatus::Stopped;
                            }
                        }
                    }
                }
                _ = interval_tick_or_sleep(&mut interval) => {
                    tracing::debug!("Ticked");
                    let _ = self.tick().await;
                }
            }
        }
    }

    #[instrument(name = "RepoDiffer::tick", skip(self), fields(key = %self.key))]
    async fn tick(&self) -> Result<(), RepoDifferError> {
        let pull_requests = self
            .az_client
            .get_open_pull_requests()
            .await
            .map_err(|_| RepoDifferError::CouldNotFetchPullRequests(self.key.clone()))?;

        let changed_pull_requests = {
            let prev_pull_requests = self.prev_pull_requests.read().await;
            match prev_pull_requests.clone() {
                Some(prev_pull_requests) => pull_requests
                    .clone()
                    .into_iter()
                    .filter(|pr| !prev_pull_requests.contains(pr))
                    .collect::<Vec<PullRequest>>(),
                None => pull_requests.clone(),
            }
        };

        tracing::debug!(
            "Found {} changed pull requests: [{}]",
            changed_pull_requests.len(),
            changed_pull_requests
                .iter()
                .map(|pr| pr.title.clone())
                .collect::<Vec<String>>()
                .join(", ")
        );

        self.prev_pull_requests
            .write()
            .await
            .replace(pull_requests.clone());
        self.last_updated
            .write()
            .await
            .replace(OffsetDateTime::now_utc());

        Ok(())
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
        tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
    }
}
