use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct ProjectSearchItem {
    pub id: i64,
    #[serde(rename(deserialize = "userid"))]
    pub user_id: String,
    #[serde(rename(deserialize = "projectid"))]
    pub project_id: String,
    #[serde(rename(deserialize = "projectname"))]
    pub project_name: String,
    #[serde(rename(deserialize = "projectnr"))]
    pub project_nr: Value,
    #[serde(rename(deserialize = "leadername"))]
    pub leader_name: String,
    #[serde(rename(deserialize = "planningtype"))]
    pub planning_type: i64,
    #[serde(rename(deserialize = "isfavorite"))]
    pub is_favorite: bool,
    #[serde(rename(deserialize = "customernames"))]
    pub customer_names: String,
    #[serde(rename(deserialize = "ismember"))]
    pub is_member: bool,
    #[serde(rename(deserialize = "isleader"))]
    pub is_leader: bool,
}
