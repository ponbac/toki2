use std::time::Duration;

use az_devops::{PullRequest, RepoClient};

use super::RepoKey;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Could not fetch pull requests for repo '{0}'")]
    CouldNotFetchPullRequests(RepoKey),
}

pub struct Worker {
    pub key: RepoKey,
    pub az_client: RepoClient,
    pub prev_pull_requests: Vec<PullRequest>,
    pub refresh_interval: Duration,
    pub status: WorkerStatus,
}

impl Worker {
    pub async fn new(
        key: RepoKey,
        az_client: RepoClient,
        refresh_interval: Duration,
    ) -> Result<Self, WorkerError> {
        let prev_pull_requests = az_client
            .get_open_pull_requests()
            .await
            .map_err(|_| WorkerError::CouldNotFetchPullRequests(key.clone()))?;

        Ok(Self {
            key,
            az_client,
            prev_pull_requests,
            refresh_interval,
            status: WorkerStatus::Stopped,
        })
    }

    pub async fn run(&mut self) {
        self.status = WorkerStatus::Running;

        loop {
            if self.status == WorkerStatus::Stopped {
                break;
            }

            match self.az_client.get_open_pull_requests().await {
                Ok(pull_requests) => {
                    let new_pull_requests = pull_requests
                        .iter()
                        .filter(|pr| !self.prev_pull_requests.contains(pr))
                        .collect::<Vec<_>>();

                    if !new_pull_requests.is_empty() {
                        tracing::info!(
                            "Found {} new pull requests for repo '{}'",
                            new_pull_requests.len(),
                            self.key
                        );
                    }

                    self.prev_pull_requests = pull_requests;
                }
                Err(err) => {
                    tracing::error!(
                        "Failed to fetch pull requests for repo '{}': {}",
                        self.key,
                        err
                    );
                }
            }

            tokio::time::sleep(self.refresh_interval).await;
        }
    }

    pub fn stop(&mut self) {
        self.status = WorkerStatus::Stopped;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStatus {
    Running,
    Stopped,
}
