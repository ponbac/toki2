use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct TimerRegistration {
    pub timerregistrationid: String,
    pub projectregistrationid: String,
    pub userid: String,
    pub projectid: String,
    pub activity: String,
    pub phaseid: String,
    pub planningtaskid: Value,
    pub starttime: String,
    pub usernote: String,
    pub ticketdata: Value,
    pub internalnote: Value,
    #[serde(rename = "typeof")]
    pub typeof_field: Value,
    pub attendencelogid: String,
    pub variationid: Value,
    pub projtimehh: Value,
    pub projtimemm: Value,
    pub difference: String,
    pub projectname: String,
    pub activityname: String,
    pub attributevalue: Value,
    pub requirenote: Value,
    pub favoritetype: i64,
    pub projectnr: Value,
    pub hours: i64,
    pub seconds: i64,
    pub minutes: i64,
    pub projectregistration: TimerProjectRegistration,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerRegistrationPayload {
    pub absencetype: Value,
    pub attestlevel: i64,
    pub favoritetype: i64,
    pub phaseid: String,
    pub phasename: String,
    pub planningtaskid: Value,
    pub planningtype: i64,
    pub projectnr: String,
    pub regday: String,
    pub reportnr: i64,
    pub requirenote: bool,
    pub usernote: Option<String>,
    pub ticket: PayloadTicket,
    pub userid: String,
    pub variationid: String,
    pub week_number: i64,
    pub timedistributiontype: String,
    pub projtime: String,
    pub inputtime: String,
    pub internalnote: String,
    pub projectid: String,
    pub projectname: String,
    pub activity: String,
    pub activityname: String,
}

impl TimerRegistrationPayload {
    pub fn new(
        activity: String,
        activity_name: String,
        project_id: String,
        project_name: String,
        user_id: String,
        user_note: Option<String>,
        reg_day: String,
        week_number: i64,
    ) -> Self {
        Self {
            activity,
            activityname: activity_name,
            projectid: project_id,
            projectname: project_name,
            userid: user_id,
            usernote: user_note,
            regday: reg_day,
            week_number,
            inputtime: "00:00".to_string(),
            phaseid: "Default".to_string(),
            projtime: "00:00".to_string(),
            timedistributiontype: "NORMALTIME".to_string(),
            ..Self::default()
        }
    }
}

pub struct StartTimerOptions {
    pub activity: String,
    pub activity_name: String,
    pub project_id: String,
    pub project_name: String,
    pub user_id: String,
    pub user_note: Option<String>,
    pub reg_day: String,
    pub week_number: i64,
}

impl StartTimerOptions {
    pub fn new(
        activity: String,
        activity_name: String,
        project_id: String,
        project_name: String,
        user_id: String,
        user_note: Option<String>,
        reg_day: String,
        week_number: i64,
    ) -> Self {
        Self {
            activity,
            activity_name,
            project_id,
            project_name,
            user_id,
            user_note,
            reg_day,
            week_number,
        }
    }
}

impl From<StartTimerOptions> for TimerRegistrationPayload {
    fn from(start_timer_options: StartTimerOptions) -> Self {
        TimerRegistrationPayload::new(
            start_timer_options.activity,
            start_timer_options.activity_name,
            start_timer_options.project_id,
            start_timer_options.project_name,
            start_timer_options.user_id,
            start_timer_options.user_note,
            start_timer_options.reg_day,
            start_timer_options.week_number,
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadTicket {}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename(serialize = "camelCase"))]
pub struct TimerProjectRegistration {
    #[serde(rename(deserialize = "timerregistrationid"))]
    pub timer_registration_id: String,
    #[serde(rename(deserialize = "projectregistrationid"))]
    pub project_registration_id: String,
    #[serde(rename(deserialize = "userid"))]
    pub user_id: String,
    #[serde(rename(deserialize = "projectid"))]
    pub project_id: String,
    pub activity: String,
    #[serde(rename(deserialize = "phaseid"))]
    pub phase_id: String,
    #[serde(rename(deserialize = "planningtaskid"))]
    pub planning_task_id: Value,
    #[serde(rename(deserialize = "starttime"))]
    pub start_time: String,
    #[serde(rename(deserialize = "usernote"))]
    pub user_note: String,
    #[serde(rename(deserialize = "ticketdata"))]
    pub ticket_data: Value,
    #[serde(rename(deserialize = "internalnote"))]
    pub internal_note: Value,
    #[serde(rename(deserialize = "typeof"))]
    pub typeof_field: Value,
    #[serde(rename(deserialize = "attendencelogid"))]
    pub attendence_log_id: String,
    #[serde(rename(deserialize = "variationid"))]
    pub variation_id: Value,
    #[serde(rename(deserialize = "projtimehh"))]
    pub proj_time_hh: Value,
    #[serde(rename(deserialize = "projtimemm"))]
    pub proj_time_mm: Value,
    pub difference: String,
    #[serde(rename(deserialize = "projectname"))]
    pub project_name: String,
    #[serde(rename(deserialize = "activityname"))]
    pub activity_name: String,
    #[serde(rename(deserialize = "attributevalue"))]
    pub attribute_value: Value,
    #[serde(rename(deserialize = "requirenote"))]
    pub require_note: Value,
    #[serde(rename(deserialize = "favoritetype"))]
    pub favorite_type: i64,
    #[serde(rename(deserialize = "projectnr"))]
    pub project_nr: Value,
    pub hours: i64,
    pub seconds: i64,
    pub minutes: i64,
    pub ticket: PayloadTicket,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub ticketdata: Value,
}
