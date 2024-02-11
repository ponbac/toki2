use std::error::Error;

use azure_devops_rust_api::git::models::{
    git_pull_request::{MergeFailureType, MergeStatus, Status},
    GitCommitRef, GitPullRequest, GitPullRequestCompletionOptions,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::RepoClient;

use super::identity::{Identity, IdentityWithVote};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    pub status: Status,
    pub created_by: Identity,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub closed_at: Option<OffsetDateTime>,
    pub auto_complete_set_by: Option<Identity>,
    pub completion_options: Option<GitPullRequestCompletionOptions>,
    pub is_draft: bool,
    pub merge_status: Option<MergeStatus>,
    pub merge_job_id: Option<String>,
    pub merge_failure_type: Option<MergeFailureType>,
    pub merge_failure_message: Option<String>,
    pub reviewers: Vec<IdentityWithVote>,
    pub url: String,
}

impl PullRequest {
    pub async fn threads(&self, client: &RepoClient) -> Result<Vec<crate::Thread>, Box<dyn Error>> {
        client.get_threads_in_pull_request(self.id).await
    }

    pub async fn commits(&self, client: &RepoClient) -> Result<Vec<GitCommitRef>, Box<dyn Error>> {
        client.get_commits_in_pull_request(self.id).await
    }

    pub async fn work_items(
        &self,
        client: &RepoClient,
    ) -> Result<Vec<crate::WorkItem>, Box<dyn Error>> {
        let ids = client
            .get_work_item_ids_in_pull_request(self.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get work item ids in pull request: {}", e);
                e
            })?;

        if ids.is_empty() {
            return Ok(vec![]);
        }

        client.get_work_items(ids).await
    }
}

impl From<GitPullRequest> for PullRequest {
    fn from(pr: GitPullRequest) -> Self {
        Self {
            id: pr.pull_request_id,
            title: pr.title.unwrap(),
            description: pr.description,
            source_branch: pr.source_ref_name,
            target_branch: pr.target_ref_name,
            status: pr.status,
            created_by: pr.created_by.into(),
            created_at: pr.creation_date,
            closed_at: pr.closed_date,
            auto_complete_set_by: pr.auto_complete_set_by.map(|identity| identity.into()),
            completion_options: pr.completion_options,
            is_draft: pr.is_draft,
            merge_status: pr.merge_status,
            merge_job_id: pr.merge_id,
            merge_failure_type: pr.merge_failure_type,
            merge_failure_message: pr.merge_failure_message,
            reviewers: pr
                .reviewers
                .into_iter()
                .map(IdentityWithVote::from)
                .collect(),
            url: pr.url,
        }
    }
}
