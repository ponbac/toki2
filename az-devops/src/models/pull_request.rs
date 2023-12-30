use azure_devops_rust_api::git::models::{git_pull_request::Status, GitPullRequest};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::identity::Identity;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullRequest {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    pub status: Status,
    pub created_by: Identity,
    pub created_at: OffsetDateTime,
    pub closed_at: Option<OffsetDateTime>,
    pub url: String,
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
            url: pr.url,
        }
    }
}
