use serde::{Deserialize, Serialize};

use super::RepoKey;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    #[serde(flatten)]
    pub pull_request_base: az_devops::PullRequest,
    pub threads: Vec<az_devops::Thread>,
    pub commits: Vec<az_devops::GitCommitRef>,
    pub work_items: Vec<az_devops::WorkItem>,
}

impl From<&PullRequest> for RepoKey {
    fn from(pr: &PullRequest) -> Self {
        Self::new(&pr.organization, &pr.project, &pr.repo_name)
    }
}
