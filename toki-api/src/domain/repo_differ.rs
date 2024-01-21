use core::fmt;
use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{PullRequest, RepoClient};
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

pub enum RepoDifferMessage {
    Start(Duration),
    Stop,
}

#[derive(Clone)]
pub struct RepoDiffer {
    key: RepoKey,
    az_client: RepoClient,
    prev_pull_requests: Option<Vec<PullRequest>>,
    last_updated: Option<OffsetDateTime>,
    pr_cache: Arc<RwLock<HashMap<RepoKey, Vec<PullRequest>>>>,
}

impl RepoDiffer {
    pub fn new(
        key: RepoKey,
        az_client: RepoClient,
        pr_cache: Arc<RwLock<HashMap<RepoKey, Vec<PullRequest>>>>,
    ) -> Self {
        Self {
            key,
            az_client,
            prev_pull_requests: None,
            last_updated: None,
            pr_cache,
        }
    }
}

impl RepoDiffer {
    #[instrument(name = "RepoDiffer::run", skip(self, receiver), fields(key = %self.key))]
    pub async fn run(&mut self, mut receiver: mpsc::Receiver<RepoDifferMessage>) {
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
                        }
                        RepoDifferMessage::Stop => {
                            interval = None;
                        }
                    }
                }
                _ = interval_tick_or_sleep(&mut interval) => {
                    tracing::debug!("Ticked");
                    self.tick().await;
                }
            }
        }
    }

    #[instrument(name = "RepoDiffer::tick", skip(self), fields(key = %self.key))]
    async fn tick(&mut self) {
        let pull_requests = self
            .az_client
            .get_open_pull_requests()
            .await
            .expect("Could not fetch pull requests");

        let changed_pull_requests = match &self.prev_pull_requests {
            Some(prev_pull_requests) => pull_requests
                .clone()
                .into_iter()
                .filter(|pr| !prev_pull_requests.contains(pr))
                .collect::<Vec<PullRequest>>(),
            None => pull_requests.clone(),
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

        self.prev_pull_requests = Some(pull_requests.clone());
        self.last_updated = Some(OffsetDateTime::now_utc());
        let mut pr_cache = self.pr_cache.write().await;
        pr_cache.insert(self.key.clone(), pull_requests);
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
