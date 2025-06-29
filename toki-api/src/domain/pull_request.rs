use az_devops::{IdentityWithVote, ThreadStatus, Vote};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::domain::Email;

use super::{PRChangeEvent, RepoKey};

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

impl PullRequest {
    pub fn new(
        key: &RepoKey,
        pull_request_base: az_devops::PullRequest,
        threads: Vec<az_devops::Thread>,
        commits: Vec<az_devops::GitCommitRef>,
        work_items: Vec<az_devops::WorkItem>,
    ) -> Self {
        Self {
            organization: key.organization.clone(),
            project: key.project.clone(),
            repo_name: key.repo_name.clone(),
            pull_request_base,
            threads,
            commits,
            work_items,
        }
    }

    pub fn azure_url(&self) -> String {
        format!(
            "https://dev.azure.com/{}/{}/_git/{}/pullrequest/{}",
            self.organization, self.project, self.repo_name, self.pull_request_base.id
        )
    }

    pub fn with_replaced_mentions(&self, id_to_email_map: &HashMap<String, String>) -> Self {
        let mut pr = self.clone();
        pr.threads = pr
            .threads
            .iter()
            .map(|t| t.with_replaced_mentions(id_to_email_map))
            .collect();
        pr
    }

    pub fn changelog(
        &self,
        new: Option<&Self>,
        id_to_email_map: &HashMap<String, Email>,
    ) -> PullRequestDiff {
        let new_pr = match new {
            Some(new) => new,
            None => return (self.clone(), vec![PRChangeEvent::PullRequestClosed]).into(),
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

                // New comments in the thread
                old_thread.is_some_and(|ot| t.comments.len() > ot.comments.len())
            })
            .map(|thread| PRChangeEvent::ThreadUpdated(thread.clone()));

        // Detect mentions in new comments
        let mention_events = new_pr
            .threads
            .iter()
            .flat_map(|new_thread| {
                let old_thread = self.threads.iter().find(|ot| ot.id == new_thread.id);

                // Get new comments (either all comments if thread is new, or only new comments if thread existed)
                let new_comments: Vec<&az_devops::Comment> = match old_thread {
                    Some(old_thread) => new_thread
                        .comments
                        .iter()
                        .skip(old_thread.comments.len())
                        .collect(),
                    None => new_thread.comments.iter().collect(),
                };

                // For each new comment, create mention events for each mention
                new_comments
                    .into_iter()
                    .filter(|comment| !comment.is_system_comment())
                    .flat_map(|comment| {
                        comment
                            .mentions()
                            .into_iter()
                            .filter_map(move |mention_id| {
                                // Resolve mention ID to email
                                id_to_email_map.get(&mention_id).map(|email| {
                                    PRChangeEvent::CommentMentioned(comment.clone(), email.clone())
                                })
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut change_events = Vec::new();
        change_events.extend(new_threads);
        change_events.extend(updated_threads);
        change_events.extend(mention_events);
        (new_pr.clone(), change_events).into()
    }

    /// Returns the identities that are blocking this PR.
    ///
    /// A PR is blocked if it has a reviewer that has voted Rejected or WaitingForAuthor,
    /// or if it has an unresolved thread.
    pub fn blocked_by(&self, threads: &[az_devops::Thread]) -> Vec<az_devops::IdentityWithVote> {
        let rejected_or_waiting = self
            .pull_request_base
            .reviewers
            .iter()
            .filter(|r| matches!(r.vote, Some(Vote::Rejected) | Some(Vote::WaitingForAuthor)))
            .cloned()
            .collect::<Vec<_>>();
        let unresolved_thread_authors = threads
            .iter()
            .filter(|t| t.status == Some(ThreadStatus::Active) && !t.is_system_thread())
            .filter_map(|t| {
                t.comments
                    .iter()
                    .find(|c| c.is_deleted != Some(true) && !c.is_system_comment())
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
                && !self.pull_request_base.reviewers.iter().any(|r| {
                    r.identity.id == author.identity.id
                        && r.vote == Some(Vote::ApprovedWithSuggestions)
                })
            {
                blocking_authors.push(author);
            }
        }

        blocking_authors
    }

    pub fn approved_by(&self) -> Vec<az_devops::IdentityWithVote> {
        let blocked_by = self.blocked_by(&self.threads);
        self.pull_request_base
            .reviewers
            .iter()
            .filter(|r| {
                matches!(
                    r.vote,
                    Some(Vote::Approved) | Some(Vote::ApprovedWithSuggestions)
                ) && !blocked_by.iter().any(|b| b.identity.id == r.identity.id)
            })
            .cloned()
            .collect()
    }

    /// Returns whether the PR is waiting for user to review and whether the review is required.
    pub fn waiting_for_user_review(&self, user_email: &str) -> (bool, bool) {
        let waiting_for_user_review = self.pull_request_base.reviewers.iter().find(|reviewer| {
            reviewer.identity.unique_name == user_email
                && reviewer.vote == Some(Vote::NoResponse)
                && !self.pull_request_base.is_draft
                && self.pull_request_base.created_by.unique_name != user_email
                && !self
                    .blocked_by(&self.threads)
                    .iter()
                    .any(|b| b.identity.id == reviewer.identity.id)
        });

        (
            waiting_for_user_review.is_some(),
            waiting_for_user_review.is_some_and(|r| r.is_required.unwrap_or_default()),
        )
    }
}

#[derive(Debug, Clone)]
pub struct PullRequestDiff {
    pub pr: PullRequest,
    pub changes: Vec<PRChangeEvent>,
}

impl PullRequestDiff {
    pub fn new(pr: PullRequest, changes: Vec<PRChangeEvent>) -> Self {
        Self { pr, changes }
    }
}

impl From<(PullRequest, Vec<PRChangeEvent>)> for PullRequestDiff {
    fn from((pr, changes): (PullRequest, Vec<PRChangeEvent>)) -> Self {
        Self::new(pr, changes)
    }
}

impl From<&PullRequest> for RepoKey {
    fn from(pr: &PullRequest) -> Self {
        Self::new(&pr.organization, &pr.project, &pr.repo_name)
    }
}
