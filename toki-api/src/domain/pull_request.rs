use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{Email, PRChangeEvent, RepoKey};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestIdentity {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PullRequestVote {
    NoResponse,
    Approved,
    ApprovedWithSuggestions,
    WaitingForAuthor,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewer {
    pub identity: PullRequestIdentity,
    pub vote: Option<PullRequestVote>,
    pub has_declined: Option<bool>,
    pub is_required: Option<bool>,
    pub is_flagged: Option<bool>,
}

impl From<PullRequestIdentity> for PullRequestReviewer {
    fn from(identity: PullRequestIdentity) -> Self {
        Self {
            identity,
            vote: None,
            has_declined: None,
            is_required: None,
            is_flagged: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestMergeStatus {
    NotSet,
    Queued,
    Conflicts,
    Succeeded,
    RejectedByPolicy,
    Failure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestCommentType {
    Unknown,
    Text,
    CodeChange,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestComment {
    pub id: i64,
    pub author: PullRequestIdentity,
    pub content: Option<String>,
    pub comment_type: Option<PullRequestCommentType>,
    #[serde(skip)]
    pub is_system: bool,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
    pub liked_by: Vec<PullRequestIdentity>,
    /// Canonical provider URL for this exact comment.
    #[serde(skip)]
    pub url: String,
}

impl PullRequestComment {
    pub fn is_system_comment(&self) -> bool {
        self.is_system
    }

    pub fn mentions(&self) -> Vec<String> {
        self.content
            .as_deref()
            .map(find_mention_ids)
            .unwrap_or_default()
    }

    fn with_replaced_mentions(&self, name_map: &HashMap<String, &str>) -> Self {
        let content = self
            .content
            .as_deref()
            .map(|content| replace_mentions(content, name_map));

        Self {
            content,
            ..self.clone()
        }
    }
}

fn find_mention_ids(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut remainder = content;

    while let Some((_, after_start)) = remainder.split_once("@<") {
        let Some((id, after_end)) = after_start.split_once('>') else {
            break;
        };
        ids.push(id.to_uppercase());
        remainder = after_end;
    }

    ids
}

fn replace_mentions(content: &str, name_map: &HashMap<String, &str>) -> String {
    let mut rendered = String::with_capacity(content.len());
    let mut remainder = content;

    while let Some((before, after_start)) = remainder.split_once("@<") {
        rendered.push_str(before);
        let Some((id, after_end)) = after_start.split_once('>') else {
            rendered.push_str("@<");
            rendered.push_str(after_start);
            return rendered;
        };

        rendered.push_str("@<");
        rendered.push_str(name_map.get(&id.to_uppercase()).copied().unwrap_or(id));
        rendered.push('>');
        remainder = after_end;
    }

    rendered.push_str(remainder);
    rendered
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestThreadStatus {
    Unknown,
    Active,
    Fixed,
    WontFix,
    Closed,
    ByDesign,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThread {
    pub id: i32,
    pub comments: Vec<PullRequestComment>,
    pub status: Option<PullRequestThreadStatus>,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub published_at: OffsetDateTime,
}

impl PullRequestThread {
    pub fn is_system_thread(&self) -> bool {
        self.comments
            .first()
            .is_some_and(PullRequestComment::is_system_comment)
    }

    pub fn author(&self) -> &PullRequestIdentity {
        &self
            .comments
            .first()
            .expect("pull request thread should contain a comment")
            .author
    }

    pub fn most_recent_comment(&self) -> &PullRequestComment {
        self.comments
            .last()
            .expect("pull request thread should contain a comment")
    }

    fn with_replaced_mentions(&self, name_map: &HashMap<String, &str>) -> Self {
        Self {
            comments: self
                .comments
                .iter()
                .map(|comment| comment.with_replaced_mentions(name_map))
                .collect(),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestWorkItemRef {
    pub id: String,
    pub title: String,
    pub url: String,
    pub parent_id: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommitAuthor {
    #[serde(with = "time::serde::rfc3339::option")]
    pub date: Option<OffsetDateTime>,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommit {
    pub author: Option<PullRequestCommitAuthor>,
    pub comment: Option<String>,
    pub commit_id: Option<String>,
    pub committer: Option<PullRequestCommitAuthor>,
    pub url: Option<String>,
}

/// Provider-neutral snapshot used by change detection and HTTP projections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub url: String,
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub created_by: PullRequestIdentity,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub source_branch: String,
    pub target_branch: String,
    pub is_draft: bool,
    pub merge_status: Option<PullRequestMergeStatus>,
    pub reviewers: Vec<PullRequestReviewer>,
    pub threads: Vec<PullRequestThread>,
    pub commits: Vec<PullRequestCommit>,
    pub work_items: Vec<PullRequestWorkItemRef>,
}

impl PullRequest {
    pub fn with_replaced_mentions(&self, id_to_name_map: &HashMap<String, String>) -> Self {
        let normalized_name_map = id_to_name_map
            .iter()
            .map(|(id, name)| (id.to_uppercase(), name.as_str()))
            .collect::<HashMap<_, _>>();
        Self {
            threads: self
                .threads
                .iter()
                .map(|thread| thread.with_replaced_mentions(&normalized_name_map))
                .collect(),
            ..self.clone()
        }
    }

    pub fn changelog(
        &self,
        repository: &RepoKey,
        new: Option<&Self>,
        id_to_email_map: &HashMap<String, Email>,
    ) -> PullRequestDiff {
        let new_pr = match new {
            Some(new) => new,
            None => {
                return PullRequestDiff::new(
                    repository.clone(),
                    self.clone(),
                    vec![PRChangeEvent::PullRequestClosed],
                );
            }
        };

        let new_threads = new_pr
            .threads
            .iter()
            .filter(|thread| {
                !self.threads.iter().any(|old| old.id == thread.id)
                    && !thread.comments.is_empty()
                    && !thread.is_system_thread()
            })
            .map(|thread| PRChangeEvent::ThreadAdded(thread.clone()));

        let updated_threads = new_pr
            .threads
            .iter()
            .filter(|thread| {
                self.threads
                    .iter()
                    .find(|old| old.id == thread.id)
                    .is_some_and(|old| thread.comments.len() > old.comments.len())
            })
            .map(|thread| PRChangeEvent::ThreadUpdated(thread.clone()));

        let mention_events = new_pr
            .threads
            .iter()
            .flat_map(|new_thread| {
                let old_thread = self.threads.iter().find(|old| old.id == new_thread.id);
                let new_comments = match old_thread {
                    Some(old_thread) => new_thread
                        .comments
                        .iter()
                        .skip(old_thread.comments.len())
                        .collect::<Vec<_>>(),
                    None => new_thread.comments.iter().collect::<Vec<_>>(),
                };

                new_comments
                    .into_iter()
                    .filter(|comment| !comment.is_system_comment())
                    .flat_map(|comment| {
                        comment
                            .mentions()
                            .into_iter()
                            .filter_map(move |mention_id| {
                                id_to_email_map.get(&mention_id).map(|email| {
                                    PRChangeEvent::CommentMentioned {
                                        comment: comment.clone(),
                                        mentioned_email: email.clone(),
                                    }
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
        PullRequestDiff::new(repository.clone(), new_pr.clone(), change_events)
    }

    /// Returns reviewers and discussion authors currently blocking this PR.
    pub fn blocked_by(&self) -> Vec<PullRequestReviewer> {
        let rejected_or_waiting = self
            .reviewers
            .iter()
            .filter(|reviewer| {
                matches!(
                    reviewer.vote,
                    Some(PullRequestVote::Rejected | PullRequestVote::WaitingForAuthor)
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let unresolved_thread_authors = self
            .threads
            .iter()
            .filter(|thread| thread.status == Some(PullRequestThreadStatus::Active))
            .filter(|thread| !thread.is_system_thread())
            .filter_map(|thread| {
                thread.comments.iter().find(|comment| {
                    comment.is_deleted != Some(true) && !comment.is_system_comment()
                })
            })
            .map(|comment| PullRequestReviewer::from(comment.author.clone()));

        let mut blocking_authors = rejected_or_waiting;
        for author in unresolved_thread_authors {
            if !blocking_authors
                .iter()
                .any(|reviewer| reviewer.identity.id == author.identity.id)
                && !self.reviewers.iter().any(|reviewer| {
                    reviewer.identity.id == author.identity.id
                        && reviewer.vote == Some(PullRequestVote::ApprovedWithSuggestions)
                })
            {
                blocking_authors.push(author);
            }
        }

        blocking_authors
    }

    pub fn approved_by(&self) -> Vec<PullRequestReviewer> {
        let blocked_by = self.blocked_by();
        self.reviewers
            .iter()
            .filter(|reviewer| {
                matches!(
                    reviewer.vote,
                    Some(PullRequestVote::Approved | PullRequestVote::ApprovedWithSuggestions)
                ) && !blocked_by
                    .iter()
                    .any(|blocked| blocked.identity.id == reviewer.identity.id)
            })
            .cloned()
            .collect()
    }

    pub fn waiting_for_user_review(&self, user_email: &str) -> (bool, bool) {
        let blocked_by = self.blocked_by();
        let waiting = self.reviewers.iter().find(|reviewer| {
            reviewer.identity.unique_name == user_email
                && reviewer.vote == Some(PullRequestVote::NoResponse)
                && !self.is_draft
                && self.created_by.unique_name != user_email
                && !blocked_by
                    .iter()
                    .any(|blocked| blocked.identity.id == reviewer.identity.id)
        });

        (
            waiting.is_some(),
            waiting.is_some_and(|reviewer| reviewer.is_required.unwrap_or_default()),
        )
    }
}

#[derive(Debug, Clone)]
pub struct PullRequestDiff {
    pub repository: RepoKey,
    pub pr: PullRequest,
    pub changes: Vec<PRChangeEvent>,
}

impl PullRequestDiff {
    pub fn new(repository: RepoKey, pr: PullRequest, changes: Vec<PRChangeEvent>) -> Self {
        Self {
            repository,
            pr,
            changes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_emits_comment_mentions() {
        let thread_id = 38_706;
        let old_thread = test_thread(
            thread_id,
            vec![test_comment(1, "author@example.com", "Initial comment")],
        );
        let new_thread = test_thread(
            thread_id,
            vec![
                test_comment(1, "author@example.com", "Initial comment"),
                test_comment(2, "reviewer@example.com", "Hello @<user-123>"),
            ],
        );

        let old_pr = test_pull_request(vec![old_thread]);
        let new_pr = test_pull_request(vec![new_thread]);
        let id_to_email = HashMap::from([(
            "USER-123".to_string(),
            Email::try_from("mentioned@example.com").unwrap(),
        )]);

        let diff = old_pr.changelog(&test_repository(), Some(&new_pr), &id_to_email);

        assert_eq!(diff.repository, test_repository());
        assert!(diff.changes.iter().any(|event| matches!(
            event,
            PRChangeEvent::CommentMentioned {
                comment,
                mentioned_email,
            } if comment.id == 2 && mentioned_email.as_ref() == "mentioned@example.com"
        )));
    }

    #[test]
    fn changelog_ignores_new_empty_threads() {
        let old_pr = test_pull_request(vec![]);
        let new_pr = test_pull_request(vec![test_thread(38_706, vec![])]);

        let diff = old_pr.changelog(&test_repository(), Some(&new_pr), &HashMap::new());

        assert!(!diff
            .changes
            .iter()
            .any(|event| matches!(event, PRChangeEvent::ThreadAdded(_))));
    }

    #[test]
    fn changelog_emits_thread_added_for_non_empty_new_threads() {
        let old_pr = test_pull_request(vec![]);
        let new_pr = test_pull_request(vec![test_thread(
            38_706,
            vec![test_comment(1, "author@example.com", "Initial comment")],
        )]);

        let diff = old_pr.changelog(&test_repository(), Some(&new_pr), &HashMap::new());

        assert!(diff.changes.iter().any(
            |event| matches!(event, PRChangeEvent::ThreadAdded(thread) if thread.id == 38_706)
        ));
    }

    fn test_pull_request(threads: Vec<PullRequestThread>) -> PullRequest {
        PullRequest {
            url: "https://example.invalid/pr/2310".to_string(),
            id: 2310,
            title: "Test PR".to_string(),
            description: None,
            created_by: test_identity("author@example.com"),
            created_at: OffsetDateTime::now_utc(),
            source_branch: "refs/heads/feature".to_string(),
            target_branch: "refs/heads/main".to_string(),
            is_draft: false,
            merge_status: None,
            reviewers: vec![],
            threads,
            commits: vec![],
            work_items: vec![],
        }
    }

    fn test_repository() -> RepoKey {
        RepoKey::new("org", "project", "repo")
    }

    fn test_thread(id: i32, comments: Vec<PullRequestComment>) -> PullRequestThread {
        PullRequestThread {
            id,
            comments,
            status: Some(PullRequestThreadStatus::Active),
            is_deleted: Some(false),
            last_updated_at: OffsetDateTime::now_utc(),
            published_at: OffsetDateTime::now_utc(),
        }
    }

    fn test_comment(id: i64, author_email: &str, content: &str) -> PullRequestComment {
        PullRequestComment {
            id,
            author: test_identity(author_email),
            content: Some(content.to_string()),
            comment_type: Some(PullRequestCommentType::Text),
            is_system: false,
            is_deleted: Some(false),
            published_at: OffsetDateTime::now_utc(),
            liked_by: vec![],
            url: format!("https://example.invalid/comments/{id}"),
        }
    }

    fn test_identity(email: &str) -> PullRequestIdentity {
        PullRequestIdentity {
            id: email.to_string(),
            display_name: email.to_string(),
            unique_name: email.to_string(),
            avatar_url: None,
        }
    }
}
