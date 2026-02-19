use azure_devops_rust_api::wit::models::{
    WorkItem as AzureWorkItem, WorkItemRelation as AzureWorkItemRelation,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::Identity;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: i32,
    pub parent_id: Option<i32>,
    pub title: String,
    pub state: String,
    pub board_column: Option<String>,
    pub item_type: String,
    pub priority: Option<i32>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub changed_at: OffsetDateTime,
    pub assigned_to: Option<Identity>,
    pub created_by: Option<Identity>,
    pub relations: Vec<WorkItemRelation>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub iteration_path: Option<String>,
    pub area_path: Option<String>,
    pub tags: Option<String>,
}

impl From<AzureWorkItem> for WorkItem {
    fn from(work_item: AzureWorkItem) -> Self {
        Self {
            id: work_item.id,
            parent_id: work_item
                .fields
                .get("System.Parent")
                .and_then(|value| value.as_i64().map(|parent_id| parent_id as i32)),
            title: work_item
                .fields
                .get("System.Title")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            state: work_item
                .fields
                .get("System.State")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            board_column: work_item
                .fields
                .get("System.BoardColumn")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
            item_type: work_item
                .fields
                .get("System.WorkItemType")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            priority: work_item
                .fields
                .get("Microsoft.VSTS.Common.Priority")
                .and_then(|value| value.as_i64().map(|p| p as i32)),
            created_at: work_item
                .fields
                .get("System.CreatedDate")
                .and_then(|value| value.as_str())
                .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
                .unwrap(),
            changed_at: work_item
                .fields
                .get("System.ChangedDate")
                .and_then(|value| value.as_str())
                .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
                .unwrap(),
            assigned_to: work_item
                .fields
                .get("System.AssignedTo")
                .and_then(|value| value.try_into().ok()),
            created_by: work_item
                .fields
                .get("System.CreatedBy")
                .and_then(|value| value.try_into().ok()),
            relations: work_item
                .relations
                .into_iter()
                .map(WorkItemRelation::from)
                .collect(),
            description: work_item
                .fields
                .get("System.Description")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
            acceptance_criteria: work_item
                .fields
                .get("Microsoft.VSTS.Common.AcceptanceCriteria")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
            iteration_path: work_item
                .fields
                .get("System.IterationPath")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
            area_path: work_item
                .fields
                .get("System.AreaPath")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
            tags: work_item
                .fields
                .get("System.Tags")
                .and_then(|value| value.as_str())
                .map(|s| s.to_owned()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkItemIdentity {
    id: String,
    display_name: String,
    unique_name: String,
    #[serde(rename = "imageUrl")]
    avatar_url: Option<String>,
}

impl From<WorkItemIdentity> for Identity {
    fn from(identity: WorkItemIdentity) -> Self {
        Self {
            id: identity.id,
            display_name: identity.display_name,
            unique_name: identity.unique_name,
            avatar_url: identity.avatar_url,
        }
    }
}

impl TryFrom<&Value> for Identity {
    type Error = serde_json::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        serde_json::from_value::<WorkItemIdentity>(value.clone()).map(Self::from)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemRelation {
    pub id: Option<i32>,
    pub name: String,
    pub relation_type: String,
    pub url: String,
}

impl From<AzureWorkItemRelation> for WorkItemRelation {
    fn from(relation: AzureWorkItemRelation) -> Self {
        let attributes = relation.link.attributes;

        Self {
            id: attributes
                .get("id")
                .and_then(|value| value.as_i64().map(|id| id as i32)),
            name: attributes
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
            relation_type: relation.link.rel,
            url: relation.link.url,
        }
    }
}

/// A comment on a work item, from the Azure DevOps Comments API.
#[derive(Clone, Debug)]
pub struct WorkItemComment {
    pub id: i32,
    /// Raw HTML text from Azure DevOps.
    pub text: String,
    pub author_name: String,
    pub created_at: OffsetDateTime,
    pub is_deleted: bool,
}

impl From<azure_devops_rust_api::wit::models::Comment> for WorkItemComment {
    fn from(c: azure_devops_rust_api::wit::models::Comment) -> Self {
        Self {
            id: c.id.unwrap_or(0),
            text: c.text.unwrap_or_default(),
            author_name: c
                .created_by
                .and_then(|a| a.graph_subject_base.display_name)
                .unwrap_or_else(|| "Unknown".to_string()),
            created_at: c.created_date.unwrap_or_else(OffsetDateTime::now_utc),
            is_deleted: c.is_deleted.unwrap_or(false),
        }
    }
}
