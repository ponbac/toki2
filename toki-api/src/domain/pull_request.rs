use az_devops::{CommentType, IdentityWithVote, ThreadStatus, Vote};
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
    pub blocked_by: Vec<az_devops::IdentityWithVote>,
}

impl PullRequest {
    pub fn new(
        key: &RepoKey,
        pull_request_base: az_devops::PullRequest,
        threads: Vec<az_devops::Thread>,
        commits: Vec<az_devops::GitCommitRef>,
        work_items: Vec<az_devops::WorkItem>,
    ) -> Self {
        let blocked_by = blocked_by(&pull_request_base, &threads);

        Self {
            organization: key.organization.clone(),
            project: key.project.clone(),
            repo_name: key.repo_name.clone(),
            pull_request_base,
            threads,
            commits,
            work_items,
            blocked_by,
        }
    }
}

fn blocked_by(
    pr: &az_devops::PullRequest,
    threads: &[az_devops::Thread],
) -> Vec<az_devops::IdentityWithVote> {
    let rejected_or_waiting = pr
        .reviewers
        .iter()
        .filter(|r| matches!(r.vote, Some(Vote::Rejected) | Some(Vote::WaitingForAuthor)))
        .cloned()
        .collect::<Vec<_>>();
    let unresolved_thread_authors = threads
        .iter()
        .filter(|t| t.status == Some(ThreadStatus::Active))
        .filter_map(|t| {
            t.comments.iter().find(|c| {
                c.is_deleted != Some(true)
                    && c.comment_type != Some(CommentType::System)
                    && c.author.display_name != "Azure Pipelines Test Service"
            })
        })
        .map(|c| c.author.clone())
        .map(IdentityWithVote::from)
        .collect::<Vec<_>>();

    // add unresolved_thread_authors to rejected_or_waiting if they are not already there
    let mut blocking_authors = rejected_or_waiting;
    for author in unresolved_thread_authors {
        if !blocking_authors
            .iter()
            .any(|r| r.identity.id == author.identity.id)
        {
            blocking_authors.push(author);
        }
    }

    blocking_authors
}

impl From<&PullRequest> for RepoKey {
    fn from(pr: &PullRequest) -> Self {
        Self::new(&pr.organization, &pr.project, &pr.repo_name)
    }
}
