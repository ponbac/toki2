use core::fmt;
use std::time::Duration;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{PullRequest, RepoClient};
use crossbeam::channel::Receiver;
use time::OffsetDateTime;

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
    pub key: RepoKey,
    pub az_client: RepoClient,
    pub prev_pull_requests: Option<Vec<PullRequest>>,
    pub last_updated: Option<OffsetDateTime>,
}

impl RepoDiffer {
    pub fn new(key: RepoKey, az_client: RepoClient) -> Self {
        Self {
            key,
            az_client,
            prev_pull_requests: None,
            last_updated: None,
        }
    }
}

impl RepoDiffer {
    pub async fn run(&mut self, reciever: Receiver<RepoDifferMessage>) {
        let mut interval = None;

        loop {
            match reciever.recv() {
                Ok(RepoDifferMessage::Start(duration)) => {
                    if interval.is_none() {
                        interval = Some(tokio::time::interval(duration));
                    }
                }
                Ok(RepoDifferMessage::Stop) => {
                    interval = None;
                }
                Err(_) => {
                    tracing::error!("Failed to receive message");
                    break;
                }
            }

            if let Some(interval) = &mut interval {
                interval.tick().await;
                tracing::debug!("Ticked");
                self.tick().await;
            }
        }
    }

    async fn tick(&mut self) {
        let pull_requests = self
            .az_client
            .get_open_pull_requests()
            .await
            .expect("Could not fetch pull requests");

        self.prev_pull_requests = Some(pull_requests.clone());
        self.last_updated = Some(OffsetDateTime::now_utc());

        let changed_pull_requests = match &self.prev_pull_requests {
            Some(prev_pull_requests) => pull_requests
                .into_iter()
                .filter(|pr| !prev_pull_requests.contains(pr))
                .collect::<Vec<PullRequest>>(),
            None => pull_requests,
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
