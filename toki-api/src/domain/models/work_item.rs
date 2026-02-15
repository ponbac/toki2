use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A provider-agnostic work item.
///
/// IDs are strings to support both Azure DevOps numeric IDs ("12345")
/// and future GitHub Issues ("owner/repo#42").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub board_state: BoardState,
    pub board_column_id: Option<String>,
    pub board_column_name: Option<String>,
    pub category: WorkItemCategory,
    /// The original state string from the provider (e.g. "Active", "New").
    pub state_name: String,
    pub priority: Option<i32>,
    pub assigned_to: Option<WorkItemPerson>,
    pub created_by: Option<WorkItemPerson>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub iteration_path: Option<String>,
    pub area_path: Option<String>,
    pub tags: Vec<String>,
    pub parent: Option<WorkItemRef>,
    pub related: Vec<WorkItemRef>,
    pub pull_requests: Vec<PullRequestRef>,
    pub url: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub changed_at: OffsetDateTime,
}

/// Simplified board column state.
///
/// Providers map their specific states (e.g. ADO "Active", GitHub "open")
/// to one of these three columns in the adapter layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum BoardState {
    Todo,
    InProgress,
    Done,
}

impl BoardState {
    /// Map a sprint taskboard column name to a board state.
    ///
    /// This covers common ADO taskboard column names. Unknown columns
    /// default to `InProgress` since most custom columns represent active work.
    pub fn from_taskboard_column(column: &str) -> Self {
        match column {
            "New" | "Proposed" | "To Do" | "Approved" | "Ready for development" => Self::Todo,
            "Done" | "Closed" | "Completed" | "Removed" => Self::Done,
            _ => Self::InProgress,
        }
    }

    /// Legacy column ID used in fallback mode.
    pub fn fallback_column_id(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "inProgress",
            Self::Done => "done",
        }
    }

    /// Legacy column display name used in fallback mode.
    pub fn fallback_column_name(self) -> &'static str {
        match self {
            Self::Todo => "To Do",
            Self::InProgress => "In Progress",
            Self::Done => "Done",
        }
    }
}

/// Build a synthetic stable column id when the provider does not supply one.
pub fn synthetic_column_id_from_name(name: &str) -> String {
    let mut id = String::with_capacity(name.len() + 5);
    id.push_str("name:");
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else {
            id.push('-');
        }
    }

    if id == "name:" {
        "name:unknown".to_string()
    } else {
        id
    }
}

/// A board column in provider-defined display order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoardColumn {
    pub id: String,
    pub name: String,
    pub order: i32,
}

/// Work item board payload (columns + items).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardData {
    pub columns: Vec<BoardColumn>,
    pub items: Vec<WorkItem>,
}

/// Column assignment for a single work item.
#[derive(Debug, Clone)]
pub struct BoardColumnAssignment {
    pub column_id: Option<String>,
    pub column_name: String,
}

/// The category/type of a work item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "String", into = "String")]
pub enum WorkItemCategory {
    UserStory,
    Bug,
    Task,
    Feature,
    Epic,
    Other(String),
}

impl From<WorkItemCategory> for String {
    fn from(value: WorkItemCategory) -> Self {
        match value {
            WorkItemCategory::UserStory => "userStory".to_string(),
            WorkItemCategory::Bug => "bug".to_string(),
            WorkItemCategory::Task => "task".to_string(),
            WorkItemCategory::Feature => "feature".to_string(),
            WorkItemCategory::Epic => "epic".to_string(),
            WorkItemCategory::Other(other) => other,
        }
    }
}

impl From<String> for WorkItemCategory {
    fn from(value: String) -> Self {
        match value.as_str() {
            "userStory" | "User Story" | "UserStory" => Self::UserStory,
            "bug" | "Bug" => Self::Bug,
            "task" | "Task" => Self::Task,
            "feature" | "Feature" => Self::Feature,
            "epic" | "Epic" => Self::Epic,
            _ => Self::Other(value),
        }
    }
}

impl std::fmt::Display for WorkItemCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserStory => write!(f, "User Story"),
            Self::Bug => write!(f, "Bug"),
            Self::Task => write!(f, "Task"),
            Self::Feature => write!(f, "Feature"),
            Self::Epic => write!(f, "Epic"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

/// A person associated with a work item (assignee, creator, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemPerson {
    pub display_name: String,
    pub unique_name: Option<String>,
    pub image_url: Option<String>,
}

/// A lightweight reference to another work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemRef {
    pub id: String,
    pub title: Option<String>,
}

/// A sprint/iteration in the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub start_date: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub finish_date: Option<OffsetDateTime>,
    pub is_current: bool,
}

/// A reference to a pull request linked to a work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRef {
    pub id: String,
    pub repository_id: String,
    pub project_id: String,
    pub url: String,
}

/// A comment on a work item (converted from provider HTML to Markdown).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemComment {
    pub id: String,
    /// Comment text as Markdown (converted from HTML).
    pub text: String,
    pub author_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A project that has work items (identified by organization + project name).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemProject {
    pub organization: String,
    pub project: String,
}

#[cfg(test)]
mod tests {
    use super::WorkItemCategory;

    #[test]
    fn work_item_category_serializes_known_variant_as_string() {
        let category = WorkItemCategory::UserStory;
        let json = serde_json::to_string(&category).expect("serialize category");
        assert_eq!(json, "\"userStory\"");
    }

    #[test]
    fn work_item_category_serializes_other_variant_as_plain_string() {
        let category = WorkItemCategory::Other("Issue".to_string());
        let json = serde_json::to_string(&category).expect("serialize category");
        assert_eq!(json, "\"Issue\"");
    }

    #[test]
    fn work_item_category_deserializes_known_and_other_values() {
        let known: WorkItemCategory =
            serde_json::from_str("\"task\"").expect("deserialize known category");
        let other: WorkItemCategory =
            serde_json::from_str("\"Issue\"").expect("deserialize other category");

        assert_eq!(known, WorkItemCategory::Task);
        assert_eq!(other, WorkItemCategory::Other("Issue".to_string()));
    }
}
