use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::models::{
    BoardColumn, BoardData, BoardState, Iteration, PullRequestRef, WorkItem, WorkItemCategory,
    WorkItemPerson, WorkItemProject, WorkItemRef,
};

/// The normalized board state for a work item.
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

/// A work item as returned by the API.
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

/// Approval summary for a pull-request work-item reference.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestApprovalStatusResponse {
    pub approved_by: Vec<PullRequestReviewerResponse>,
    pub blocked_by: Vec<PullRequestReviewerResponse>,
}

/// Reviewer identity attached to a pull-request work-item reference.
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
