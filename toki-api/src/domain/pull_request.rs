use core::fmt;

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

    pub fn changelog(&self, new: Option<&Self>) -> (Self, Vec<PRChangeEvent>) {
        let new_pr = match new {
            Some(new) => new,
            None => return (self.clone(), vec![PRChangeEvent::PullRequestClosed]),
        };

        let new_threads = new_pr
            .threads
            .iter()
            .filter(|t| !self.threads.iter().any(|ot| ot.id == t.id) && !t.is_system_thread())
            .map(|thread| PRChangeEvent::ThreadAdded(thread.clone()));

        let updated_threads = new_pr
            .threads
            .iter()
            .filter(|t| {
                let old_thread = self.threads.iter().find(|ot| ot.id == t.id);

                let status_changed = old_thread.map_or(false, |ot| ot.status != t.status);
                let has_new_comment =
                    old_thread.map_or(false, |ot| t.comments.len() > ot.comments.len());

                status_changed || has_new_comment
            })
            .map(|thread| PRChangeEvent::ThreadUpdated(thread.clone()));

        (new_pr.clone(), new_threads.chain(updated_threads).collect())
    }
}

#[derive(Debug)]
pub enum PRChangeEvent {
    PullRequestClosed,
    ThreadAdded(az_devops::Thread),
    ThreadUpdated(az_devops::Thread),
}

impl fmt::Display for PRChangeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PRChangeEvent::PullRequestClosed => {
                write!(f, "PullRequestClosed")
            }
            PRChangeEvent::ThreadAdded(thread) => {
                write!(f, "ThreadAdded({})", thread.id)
            }
            PRChangeEvent::ThreadUpdated(thread) => {
                write!(f, "ThreadUpdated({})", thread.id)
            }
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

    // add unresolved_thread_authors to rejected_or_waiting if they are not already there and not approved with suggestions
    let mut blocking_authors = rejected_or_waiting;
    for author in unresolved_thread_authors {
        if !blocking_authors
            .iter()
            .any(|r| r.identity.id == author.identity.id)
            && !pr.reviewers.iter().any(|r| {
                r.identity.id == author.identity.id && r.vote == Some(Vote::ApprovedWithSuggestions)
            })
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
