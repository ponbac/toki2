use async_trait::async_trait;

use crate::domain::{PullRequest, PullRequestIdentity, RepoKey};

#[derive(Debug, thiserror::Error)]
pub enum PullRequestProviderError {
    #[error("Could not fetch pull requests")]
    PullRequests,
    #[error("Could not fetch pull request threads")]
    Threads,
    #[error("Could not fetch pull request commits")]
    Commits,
    #[error("Could not fetch linked work items")]
    WorkItems,
    #[error("Could not fetch provider identities")]
    Identities,
}

/// Outbound port for a source-control provider scoped to one repository.
///
/// Implementations return fully hydrated, provider-neutral pull requests. The
/// provider adapter owns wire-type conversion, canonical URL construction, and
/// any fan-out needed to fetch threads, commits, and linked work items.
#[async_trait]
pub trait PullRequestProvider: Send + Sync + 'static {
    /// Repository this provider instance reads from.
    fn repository(&self) -> &RepoKey;

    async fn get_open_pull_requests(&self) -> Result<Vec<PullRequest>, PullRequestProviderError>;

    async fn get_identities(&self) -> Result<Vec<PullRequestIdentity>, PullRequestProviderError>;
}
