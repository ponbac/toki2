use core::fmt;

use az_devops::{PullRequest, RepoClient};
use time::OffsetDateTime;

use super::RepoKey;

#[derive(Debug, thiserror::Error)]
pub enum RepoDifferError {
    #[error("Could not fetch pull requests for repo '{0}'")]
    CouldNotFetchPullRequests(RepoKey),
}

#[derive(Clone)]
pub struct RepoDiffer {
    pub key: RepoKey,
    pub az_client: RepoClient,
    pub prev_pull_requests: Option<Vec<PullRequest>>,
    pub last_updated: Option<OffsetDateTime>,
}

impl RepoDiffer {
    pub async fn new(key: RepoKey, az_client: RepoClient) -> Self {
        Self {
            key,
            az_client,
            prev_pull_requests: None,
            last_updated: None,
        }
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
