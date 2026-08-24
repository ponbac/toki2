use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::domain::{
    PullRequest, PullRequestComment, PullRequestCommentType, PullRequestIdentity,
    PullRequestMergeStatus, PullRequestReviewer, PullRequestThread, PullRequestThreadStatus,
    PullRequestVote, PullRequestWorkItemRef, RepoKey,
};

/// A person identity in the pull-request list.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestIdentityResponse {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
}

impl From<PullRequestIdentity> for PullRequestIdentityResponse {
    fn from(identity: PullRequestIdentity) -> Self {
        Self {
            id: identity.id,
            display_name: identity.display_name,
            unique_name: identity.unique_name,
            avatar_url: identity.avatar_url,
        }
    }
}

/// Reviewer vote on a pull request.
#[derive(Debug, Serialize, ToSchema)]
pub enum PullRequestVoteResponse {
    NoResponse,
    Approved,
    ApprovedWithSuggestions,
    WaitingForAuthor,
    Rejected,
}

impl From<PullRequestVote> for PullRequestVoteResponse {
    fn from(vote: PullRequestVote) -> Self {
        match vote {
            PullRequestVote::NoResponse => Self::NoResponse,
            PullRequestVote::Approved => Self::Approved,
            PullRequestVote::ApprovedWithSuggestions => Self::ApprovedWithSuggestions,
            PullRequestVote::WaitingForAuthor => Self::WaitingForAuthor,
            PullRequestVote::Rejected => Self::Rejected,
        }
    }
}

/// Provider-neutral merge state.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestMergeStatusResponse {
    NotSet,
    Queued,
    Conflicts,
    Succeeded,
    RejectedByPolicy,
    Failure,
}

impl From<PullRequestMergeStatus> for PullRequestMergeStatusResponse {
    fn from(status: PullRequestMergeStatus) -> Self {
        match status {
            PullRequestMergeStatus::NotSet => Self::NotSet,
            PullRequestMergeStatus::Queued => Self::Queued,
            PullRequestMergeStatus::Conflicts => Self::Conflicts,
            PullRequestMergeStatus::Succeeded => Self::Succeeded,
            PullRequestMergeStatus::RejectedByPolicy => Self::RejectedByPolicy,
            PullRequestMergeStatus::Failure => Self::Failure,
        }
    }
}

/// Kind of comment in a pull-request discussion.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestCommentTypeResponse {
    Unknown,
    Text,
    CodeChange,
    System,
}

impl From<PullRequestCommentType> for PullRequestCommentTypeResponse {
    fn from(comment_type: PullRequestCommentType) -> Self {
        match comment_type {
            PullRequestCommentType::Unknown => Self::Unknown,
            PullRequestCommentType::Text => Self::Text,
            PullRequestCommentType::CodeChange => Self::CodeChange,
            PullRequestCommentType::System => Self::System,
        }
    }
}

/// Resolution state of a pull-request discussion.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestThreadStatusResponse {
    Unknown,
    Active,
    Fixed,
    WontFix,
    Closed,
    ByDesign,
    Pending,
}

impl From<PullRequestThreadStatus> for PullRequestThreadStatusResponse {
    fn from(status: PullRequestThreadStatus) -> Self {
        match status {
            PullRequestThreadStatus::Unknown => Self::Unknown,
            PullRequestThreadStatus::Active => Self::Active,
            PullRequestThreadStatus::Fixed => Self::Fixed,
            PullRequestThreadStatus::WontFix => Self::WontFix,
            PullRequestThreadStatus::Closed => Self::Closed,
            PullRequestThreadStatus::ByDesign => Self::ByDesign,
            PullRequestThreadStatus::Pending => Self::Pending,
        }
    }
}

/// A reviewer together with their vote and flags.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewerVoteResponse {
    pub identity: PullRequestIdentityResponse,
    pub vote: Option<PullRequestVoteResponse>,
    pub has_declined: Option<bool>,
    pub is_required: Option<bool>,
    pub is_flagged: Option<bool>,
}

impl From<PullRequestReviewer> for PullRequestReviewerVoteResponse {
    fn from(reviewer: PullRequestReviewer) -> Self {
        Self {
            identity: reviewer.identity.into(),
            vote: reviewer.vote.map(Into::into),
            has_declined: reviewer.has_declined,
            is_required: reviewer.is_required,
            is_flagged: reviewer.is_flagged,
        }
    }
}

/// A comment on a pull-request thread.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommentResponse {
    pub id: i64,
    pub author: PullRequestIdentityResponse,
    pub content: Option<String>,
    /// Comment kind, for example `text` or `system`.
    pub comment_type: Option<PullRequestCommentTypeResponse>,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub published_at: OffsetDateTime,
}

impl From<PullRequestComment> for PullRequestCommentResponse {
    fn from(comment: PullRequestComment) -> Self {
        Self {
            id: comment.id,
            author: comment.author.into(),
            content: comment.content,
            comment_type: comment.comment_type.map(Into::into),
            is_deleted: comment.is_deleted,
            published_at: comment.published_at,
        }
    }
}

/// A discussion thread on a pull request.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadResponse {
    pub id: i32,
    pub comments: Vec<PullRequestCommentResponse>,
    /// Thread status, for example `active` or `fixed`.
    pub status: Option<PullRequestThreadStatusResponse>,
    pub is_deleted: Option<bool>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub last_updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub published_at: OffsetDateTime,
}

impl From<PullRequestThread> for PullRequestThreadResponse {
    fn from(thread: PullRequestThread) -> Self {
        Self {
            id: thread.id,
            comments: thread.comments.into_iter().map(Into::into).collect(),
            status: thread.status.map(Into::into),
            is_deleted: thread.is_deleted,
            last_updated_at: thread.last_updated_at,
            published_at: thread.published_at,
        }
    }
}

/// A work item linked to a pull request.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestWorkItemRefResponse {
    pub id: String,
    pub title: String,
    pub url: String,
    pub parent_id: Option<String>,
    pub priority: Option<i32>,
}

impl From<PullRequestWorkItemRef> for PullRequestWorkItemRefResponse {
    fn from(work_item: PullRequestWorkItemRef) -> Self {
        Self {
            id: work_item.id,
            title: work_item.title,
            url: work_item.url,
            parent_id: work_item.parent_id,
            priority: work_item.priority,
        }
    }
}

/// Trimmed pull request used by the agent catalog and the PR list UI.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListPullRequestResponse {
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    pub url: String,
    pub id: i32,
    pub title: String,
    pub created_by: PullRequestIdentityResponse,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub created_at: OffsetDateTime,
    pub source_branch: String,
    pub target_branch: String,
    pub is_draft: bool,
    /// Merge status, for example `succeeded` or `conflicts`.
    pub merge_status: Option<PullRequestMergeStatusResponse>,
    pub threads: Vec<PullRequestThreadResponse>,
    pub work_items: Vec<PullRequestWorkItemRefResponse>,
    pub reviewers: Vec<PullRequestReviewerVoteResponse>,
    pub blocked_by: Vec<PullRequestReviewerVoteResponse>,
    pub approved_by: Vec<PullRequestReviewerVoteResponse>,
    pub waiting_for_user_review: bool,
    pub review_required: bool,
}

impl ListPullRequestResponse {
    pub fn from_domain(repository: RepoKey, pull_request: PullRequest, user_email: &str) -> Self {
        let blocked_by = pull_request.blocked_by();
        let approved_by = pull_request.approved_by();
        let (waiting_for_user_review, review_required) =
            pull_request.waiting_for_user_review(user_email);

        Self {
            organization: repository.organization,
            project: repository.project,
            repo_name: repository.repo_name,
            url: pull_request.url,
            id: pull_request.id,
            title: pull_request.title,
            created_by: pull_request.created_by.into(),
            created_at: pull_request.created_at,
            source_branch: pull_request.source_branch,
            target_branch: pull_request.target_branch,
            is_draft: pull_request.is_draft,
            merge_status: pull_request.merge_status.map(Into::into),
            threads: pull_request.threads.into_iter().map(Into::into).collect(),
            work_items: pull_request
                .work_items
                .into_iter()
                .map(Into::into)
                .collect(),
            reviewers: pull_request.reviewers.into_iter().map(Into::into).collect(),
            blocked_by: blocked_by.into_iter().map(Into::into).collect(),
            approved_by: approved_by.into_iter().map(Into::into).collect(),
            waiting_for_user_review,
            review_required,
        }
    }
}
