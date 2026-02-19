//! HTTP response types for time tracking and work item endpoints.
//!
//! These types serialize to the JSON format expected by the frontend.

use serde::Serialize;
use time::OffsetDateTime;

use crate::domain::models::{
    ActiveTimer, Activity, AttestLevel, BoardColumn, BoardData, BoardState, Iteration, Project,
    PullRequestRef, TimeEntry, TimeInfo, TimerHistoryEntry, WorkItem, WorkItemCategory,
    WorkItemPerson, WorkItemProject, WorkItemRef,
};

/// Response for the get timer endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimerResponse {
    pub timer: Option<TimerResponse>,
}

/// Active timer response - all timers are standalone now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerResponse {
    /// When the timer was started (ISO 8601).
    #[serde(with = "time::serde::rfc3339")]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
    pub start_time: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    pub week_number: u8,
    pub attest_level: AttestLevel,
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
            attest_level: entry.attest_level,
        }
    }
}

/// Timer history entry response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerHistoryEntryResponse {
    pub id: i32,
    pub registration_id: Option<String>,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
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

/// Time info response - period statistics.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeInfoResponse {
    pub period_time_left: f64,
    pub worked_period_time: f64,
    pub scheduled_period_time: f64,
    pub worked_period_with_absence_time: f64,
    pub flex_time_current: f64,
}

impl From<TimeInfo> for TimeInfoResponse {
    fn from(info: TimeInfo) -> Self {
        Self {
            period_time_left: info.period_time_left,
            worked_period_time: info.worked_period_time,
            scheduled_period_time: info.scheduled_period_time,
            worked_period_with_absence_time: info.worked_period_with_absence_time,
            flex_time_current: info.flex_time_current,
        }
    }
}

// ---------------------------------------------------------------------------
// Work Item response types
// ---------------------------------------------------------------------------

/// A work item as returned by the API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemResponse {
    pub id: String,
    pub title: String,
    pub board_state: BoardState,
    pub board_column_id: Option<String>,
    pub board_column_name: Option<String>,
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
            board_state: item.board_state,
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestApprovalStatusResponse {
    pub approved_by: Vec<PullRequestReviewerResponse>,
    pub blocked_by: Vec<PullRequestReviewerResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewerResponse {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
}

/// Response for the format-for-llm endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatForLlmResponse {
    pub markdown: String,
    pub has_images: bool,
}

/// A sprint/iteration response.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
