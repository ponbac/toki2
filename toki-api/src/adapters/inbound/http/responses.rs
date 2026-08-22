//! HTTP response types for time tracking and work item endpoints.
//!
//! These types serialize to the JSON format expected by the frontend and the
//! agent OpenAPI catalog.

use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::domain::models::{
    AbsenceChild, AbsenceDayDefault, AbsenceEntry, AbsenceType, ActiveTimer, Activity, BoardColumn,
    BoardData, BoardState, Iteration, Project, PullRequestRef, TimeEntry, TimeEntryDayStatus,
    TimeEntryStatus, TimerHistoryEntry, WeeklyStats, WorkItem, WorkItemCategory, WorkItemPerson,
    WorkItemProject, WorkItemRef,
};
use crate::domain::{
    PullRequest, PullRequestComment, PullRequestCommentType, PullRequestIdentity,
    PullRequestMergeStatus, PullRequestReviewer, PullRequestThread, PullRequestThreadStatus,
    PullRequestVote, PullRequestWorkItemRef,
};

/// JSON error body returned by HTTP handlers.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Human-readable error message.
    pub error: String,
}

/// Response for the get timer endpoint.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetTimerResponse {
    pub timer: Option<TimerResponse>,
}

/// Response for saving the active timer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerResponse {
    pub entry: TimeEntryResponse,
    pub timer: Option<TimerResponse>,
}

/// Active timer response - all timers are standalone now.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimerResponse {
    /// When the timer was started (ISO 8601).
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub start_time: OffsetDateTime,
    /// Project ID (if set).
    pub project_id: Option<String>,
    /// Project name (if set).
    pub project_name: Option<String>,
    /// Activity ID/code (if set).
    pub activity_id: Option<String>,
    /// Activity name (if set).
    pub activity_name: Option<String>,
    /// User note.
    pub note: String,
    /// Elapsed hours.
    pub hours: i64,
    /// Elapsed minutes (within current hour).
    pub minutes: i64,
    /// Elapsed seconds (within current minute).
    pub seconds: i64,
}

impl From<ActiveTimer> for TimerResponse {
    fn from(timer: ActiveTimer) -> Self {
        let (hours, minutes, seconds) = timer.elapsed_hms();
        Self {
            start_time: timer.started_at,
            project_id: timer.project_id.map(|id| id.to_string()),
            project_name: timer.project_name,
            activity_id: timer.activity_id.map(|id| id.to_string()),
            activity_name: timer.activity_name,
            note: timer.note,
            hours,
            minutes,
            seconds,
        }
    }
}

/// Project response - simplified for frontend use.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResponse {
    pub project_id: String,
    pub project_name: String,
}

impl From<Project> for ProjectResponse {
    fn from(project: Project) -> Self {
        Self {
            project_id: project.id.to_string(),
            project_name: project.name,
        }
    }
}

/// Activity response - simplified for frontend use.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityResponse {
    /// Activity code (used in API calls).
    pub activity: String,
    /// Activity display name.
    pub activity_name: String,
}

impl From<Activity> for ActivityResponse {
    fn from(activity: Activity) -> Self {
        Self {
            activity: activity.id.to_string(),
            activity_name: activity.name,
        }
    }
}

/// Time entry response - completed time registration.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimeEntryStatusResponse {
    Open,
    Approved,
    Certified,
}

impl From<TimeEntryStatus> for TimeEntryStatusResponse {
    fn from(status: TimeEntryStatus) -> Self {
        match status {
            TimeEntryStatus::Open => Self::Open,
            TimeEntryStatus::Approved => Self::Approved,
            TimeEntryStatus::Certified => Self::Certified,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryResponse {
    pub registration_id: String,
    pub project_id: String,
    pub project_name: String,
    pub activity_id: String,
    pub activity_name: String,
    /// Date in YYYY-MM-DD format.
    pub date: String,
    pub hours: f64,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub end_time: Option<OffsetDateTime>,
    pub week_number: u8,
    /// Attestation status: `open`, `approved`, or `certified`.
    pub status: TimeEntryStatusResponse,
}

impl From<TimeEntry> for TimeEntryResponse {
    fn from(entry: TimeEntry) -> Self {
        Self {
            registration_id: entry.registration_id,
            project_id: entry.project_id.to_string(),
            project_name: entry.project_name,
            activity_id: entry.activity_id.to_string(),
            activity_name: entry.activity_name,
            date: entry.date.to_string(),
            hours: entry.hours,
            note: entry.note,
            start_time: entry.start_time,
            end_time: entry.end_time,
            week_number: entry.week_number,
            status: entry.status.into(),
        }
    }
}

/// Date-level time entry status response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntryDayStatusResponse {
    /// Date in YYYY-MM-DD format.
    pub date: String,
    /// Attestation status: `open`, `approved`, or `certified`.
    pub status: TimeEntryStatusResponse,
}

impl From<TimeEntryDayStatus> for TimeEntryDayStatusResponse {
    fn from(day_status: TimeEntryDayStatus) -> Self {
        Self {
            date: day_status.date.to_string(),
            status: day_status.status.into(),
        }
    }
}

/// Timer history entry response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimerHistoryEntryResponse {
    pub id: i32,
    pub registration_id: Option<String>,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub created_at: OffsetDateTime,
}

impl From<TimerHistoryEntry> for TimerHistoryEntryResponse {
    fn from(entry: TimerHistoryEntry) -> Self {
        Self {
            id: entry.id.as_i32(),
            registration_id: entry.registration_id,
            user_id: entry.user_id.as_i32(),
            start_time: entry.start_time,
            end_time: entry.end_time,
            project_id: entry.project_id.map(|p| p.to_string()),
            project_name: entry.project_name,
            activity_id: entry.activity_id.map(|a| a.to_string()),
            activity_name: entry.activity_name,
            note: entry.note,
            created_at: entry.created_at,
        }
    }
}

/// Weekly stats response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyStatsResponse {
    pub worked_hours: f64,
    pub scheduled_hours: f64,
    pub remaining_hours: f64,
    pub absence_hours: f64,
    pub covered_hours: f64,
    pub period_flex_hours: f64,
}

impl From<WeeklyStats> for WeeklyStatsResponse {
    fn from(info: WeeklyStats) -> Self {
        Self {
            worked_hours: info.worked_hours,
            scheduled_hours: info.scheduled_hours,
            remaining_hours: info.remaining_hours,
            absence_hours: info.absence_hours,
            covered_hours: info.covered_hours,
            period_flex_hours: info.period_flex_hours,
        }
    }
}

/// Absence entry response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceEntryResponse {
    pub absence_id: String,
    pub date: String,
    pub hours: f64,
    pub absence_type: AbsenceType,
    pub absence_type_label: &'static str,
    pub child: Option<String>,
    pub comment: Option<String>,
    pub managed: bool,
    pub deletable: bool,
}

/// Available absence type response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceTypeResponse {
    pub absence_type: AbsenceType,
    pub absence_type_label: &'static str,
}

/// Registered child available for child-related absence reporting.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceChildResponse {
    pub name: String,
    pub birth_date: Option<String>,
}

impl From<AbsenceType> for AbsenceTypeResponse {
    fn from(absence_type: AbsenceType) -> Self {
        Self {
            absence_type,
            absence_type_label: absence_type.label(),
        }
    }
}

impl From<AbsenceChild> for AbsenceChildResponse {
    fn from(child: AbsenceChild) -> Self {
        Self {
            name: child.name,
            birth_date: child.birth_date.map(|date| date.to_string()),
        }
    }
}

impl From<AbsenceEntry> for AbsenceEntryResponse {
    fn from(entry: AbsenceEntry) -> Self {
        Self {
            absence_id: entry.absence_id,
            date: entry.date.to_string(),
            hours: entry.hours,
            absence_type: entry.absence_type,
            absence_type_label: entry.absence_type.label(),
            child: entry.child,
            comment: entry.comment,
            managed: entry.managed,
            deletable: entry.deletable,
        }
    }
}

/// Default hours for one absence day.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsenceDayDefaultResponse {
    pub date: String,
    pub scheduled_hours: f64,
}

impl From<AbsenceDayDefault> for AbsenceDayDefaultResponse {
    fn from(day: AbsenceDayDefault) -> Self {
        Self {
            date: day.date.to_string(),
            scheduled_hours: day.scheduled_hours,
        }
    }
}

// ---------------------------------------------------------------------------
// Work Item response types
// ---------------------------------------------------------------------------

/// A work item as returned by the API.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum BoardStateResponse {
    Todo,
    InProgress,
    Done,
}

impl From<BoardState> for BoardStateResponse {
    fn from(state: BoardState) -> Self {
        match state {
            BoardState::Todo => Self::Todo,
            BoardState::InProgress => Self::InProgress,
            BoardState::Done => Self::Done,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemResponse {
    pub id: String,
    pub title: String,
    /// Board column grouping: `todo`, `inProgress`, or `done`.
    pub board_state: BoardStateResponse,
    pub board_column_id: Option<String>,
    pub board_column_name: Option<String>,
    /// Work item type, for example `userStory`, `bug`, or `task`.
    #[schema(value_type = String)]
    pub category: WorkItemCategory,
    pub state_name: String,
    pub priority: Option<i32>,
    pub assigned_to: Option<WorkItemPersonResponse>,
    pub created_by: Option<WorkItemPersonResponse>,
    pub description: Option<String>,
    pub description_rendered_html: Option<String>,
    pub repro_steps: Option<String>,
    pub repro_steps_rendered_html: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub iteration_path: Option<String>,
    pub area_path: Option<String>,
    pub tags: Vec<String>,
    pub parent: Option<WorkItemRefResponse>,
    pub related: Vec<WorkItemRefResponse>,
    pub pull_requests: Vec<PullRequestRefResponse>,
    pub url: String,
    pub created_at: String,
    pub changed_at: String,
}

impl From<WorkItem> for WorkItemResponse {
    fn from(item: WorkItem) -> Self {
        let format = time::format_description::well_known::Rfc3339;
        Self {
            id: item.id,
            title: item.title,
            board_state: item.board_state.into(),
            board_column_id: item.board_column_id,
            board_column_name: item.board_column_name,
            category: item.category,
            state_name: item.state_name,
            priority: item.priority,
            assigned_to: item.assigned_to.map(Into::into),
            created_by: item.created_by.map(Into::into),
            description: item.description,
            description_rendered_html: item.description_rendered_html,
            repro_steps: item.repro_steps,
            repro_steps_rendered_html: item.repro_steps_rendered_html,
            acceptance_criteria: item.acceptance_criteria,
            iteration_path: item.iteration_path,
            area_path: item.area_path,
            tags: item.tags,
            parent: item.parent.map(Into::into),
            related: item.related.into_iter().map(Into::into).collect(),
            pull_requests: item.pull_requests.into_iter().map(Into::into).collect(),
            url: item.url,
            created_at: item.created_at.format(&format).unwrap_or_default(),
            changed_at: item.changed_at.format(&format).unwrap_or_default(),
        }
    }
}

/// A board column.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoardColumnResponse {
    pub id: String,
    pub name: String,
    pub order: i32,
}

impl From<BoardColumn> for BoardColumnResponse {
    fn from(column: BoardColumn) -> Self {
        Self {
            id: column.id,
            name: column.name,
            order: column.order,
        }
    }
}

/// Board response payload (columns + items).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoardResponse {
    pub columns: Vec<BoardColumnResponse>,
    pub items: Vec<WorkItemResponse>,
}

impl From<BoardData> for BoardResponse {
    fn from(board_data: BoardData) -> Self {
        Self {
            columns: board_data.columns.into_iter().map(Into::into).collect(),
            items: board_data.items.into_iter().map(Into::into).collect(),
        }
    }
}

/// A person associated with a work item.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemPersonResponse {
    pub display_name: String,
    pub unique_name: Option<String>,
    pub image_url: Option<String>,
}

impl From<WorkItemPerson> for WorkItemPersonResponse {
    fn from(person: WorkItemPerson) -> Self {
        Self {
            display_name: person.display_name,
            unique_name: person.unique_name,
            image_url: person.image_url,
        }
    }
}

/// A lightweight reference to another work item.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemRefResponse {
    pub id: String,
    pub title: Option<String>,
}

impl From<WorkItemRef> for WorkItemRefResponse {
    fn from(ref_item: WorkItemRef) -> Self {
        Self {
            id: ref_item.id,
            title: ref_item.title,
        }
    }
}

/// A reference to a pull request linked to a work item.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRefResponse {
    pub id: String,
    pub repository_id: String,
    pub project_id: String,
    pub url: String,
    pub title: Option<String>,
    pub source_branch: Option<String>,
    pub is_draft: Option<bool>,
    pub approval_status: Option<PullRequestApprovalStatusResponse>,
}

impl From<PullRequestRef> for PullRequestRefResponse {
    fn from(pr: PullRequestRef) -> Self {
        Self {
            id: pr.id,
            repository_id: pr.repository_id,
            project_id: pr.project_id,
            url: pr.url,
            title: None,
            source_branch: None,
            is_draft: None,
            approval_status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestApprovalStatusResponse {
    pub approved_by: Vec<PullRequestReviewerResponse>,
    pub blocked_by: Vec<PullRequestReviewerResponse>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewerResponse {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
}

/// Response for the format-for-llm endpoint.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FormatForLlmResponse {
    pub markdown: String,
    pub has_images: bool,
}

/// A sprint/iteration response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IterationResponse {
    pub id: String,
    pub name: String,
    pub path: String,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub is_current: bool,
}

impl From<Iteration> for IterationResponse {
    fn from(iteration: Iteration) -> Self {
        let format = time::format_description::well_known::Rfc3339;
        Self {
            id: iteration.id,
            name: iteration.name,
            path: iteration.path,
            start_date: iteration.start_date.and_then(|d| d.format(&format).ok()),
            finish_date: iteration.finish_date.and_then(|d| d.format(&format).ok()),
            is_current: iteration.is_current,
        }
    }
}

/// A project that has work items.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemProjectResponse {
    pub organization: String,
    pub project: String,
}

impl From<WorkItemProject> for WorkItemProjectResponse {
    fn from(project: WorkItemProject) -> Self {
        Self {
            organization: project.organization,
            project: project.project,
        }
    }
}

// ---------------------------------------------------------------------------
// Pull request list response types (Toki-owned agent contract)
// ---------------------------------------------------------------------------

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
    pub fn from_domain(pull_request: PullRequest, user_email: &str) -> Self {
        let blocked_by = pull_request.blocked_by();
        let approved_by = pull_request.approved_by();
        let (waiting_for_user_review, review_required) =
            pull_request.waiting_for_user_review(user_email);

        Self {
            organization: pull_request.organization,
            project: pull_request.project,
            repo_name: pull_request.repo_name,
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
