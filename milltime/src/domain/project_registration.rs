use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ProjectRegistrationPayload {
    #[serde(rename = "absencetype")]
    pub absence_type: Option<String>,
    #[serde(rename = "attestlevel")]
    pub attest_level: i32,
    #[serde(rename = "favoritetype")]
    pub favorite_type: i32,
    #[serde(rename = "phaseid")]
    pub phase_id: String,
    #[serde(rename = "phasename")]
    pub phase_name: String,
    #[serde(rename = "planningtaskid")]
    pub planning_task_id: Option<String>,
    #[serde(rename = "planningtype")]
    pub planning_type: i32,
    #[serde(rename = "projectnr")]
    pub project_nr: String,
    #[serde(rename = "regday")]
    pub reg_day: String,
    #[serde(rename = "reportnr")]
    pub report_nr: i32,
    #[serde(rename = "requirenote")]
    pub require_note: bool,
    #[serde(rename = "ticket")]
    pub ticket: serde_json::Value,
    #[serde(rename = "userid")]
    pub user_id: String,
    #[serde(rename = "variationid")]
    pub variation_id: String,
    #[serde(rename = "weekNumber")]
    pub week_number: i32,
    #[serde(rename = "timedistributiontype")]
    pub time_distribution_type: String,
    #[serde(rename = "projtime")]
    pub proj_time: String,
    #[serde(rename = "inputtime")]
    pub input_time: String,
    #[serde(rename = "internalnote")]
    pub internal_note: String,
    #[serde(rename = "projectid")]
    pub project_id: String,
    #[serde(rename = "projectname")]
    pub project_name: String,
    #[serde(rename = "activity")]
    pub activity: String,
    #[serde(rename = "activityname")]
    pub activity_name: String,
    #[serde(rename = "usernote")]
    pub user_note: String,
}

impl ProjectRegistrationPayload {
    pub fn new(
        user_id: String,
        project_id: String,
        project_name: String,
        activity: String,
        activity_name: String,
        total_time: String,
        reg_day: String,
        week_number: i32,
        user_note: String,
    ) -> Self {
        Self {
            user_id,
            project_id,
            project_name,
            activity,
            activity_name,
            input_time: total_time.clone(),
            proj_time: total_time,
            reg_day,
            week_number,
            user_note,
            time_distribution_type: "NORMALTIME".to_string(),
            phase_id: "Default".to_string(),
            ..Default::default()
        }
    }
}
