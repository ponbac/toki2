use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivitiesRoot {
    pub phaseid: String,
    pub phasename: String,
    pub activities: Vec<Activity>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename(serialize = "camelCase"))]
pub struct Activity {
    #[serde(rename(deserialize = "userid"))]
    pub user_id: String,
    #[serde(rename(deserialize = "projectid"))]
    pub project_id: String,
    #[serde(rename(deserialize = "activity"))]
    pub activity: String,
    #[serde(rename(deserialize = "activityname"))]
    pub activity_name: String,
    #[serde(rename(deserialize = "variationid"))]
    pub variation_id: Value,
    #[serde(rename(deserialize = "absencetype"))]
    pub absence_type: Value,
    #[serde(rename(deserialize = "phaseid"))]
    pub phase_id: String,
    #[serde(rename(deserialize = "phasename"))]
    pub phase_name: String,
    #[serde(rename(deserialize = "requirenote"))]
    pub require_note: Option<bool>,
    #[serde(rename(deserialize = "phaseorder"))]
    pub phase_order: i64,
    #[serde(rename(deserialize = "isfavorite"))]
    pub is_favorite: bool,
    #[serde(rename(deserialize = "projplandescription"))]
    pub proj_plan_description: Value,
    #[serde(rename(deserialize = "planningtaskid"))]
    pub planning_task_id: Value,
    #[serde(rename(deserialize = "planningtaskname"))]
    pub planning_task_name: Value,
    #[serde(rename(deserialize = "name"))]
    pub name: String,
    #[serde(rename(deserialize = "timedistributiontype"))]
    pub time_distribution_type: Value,
    #[serde(rename(deserialize = "planningtype"))]
    pub planning_type: i64,
}
