use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AttestLevel;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct TimerRegistration {
    #[serde(rename(deserialize = "timerregistrationid"))]
    pub timer_registration_id: String,
    #[serde(rename(deserialize = "projectregistrationid"))]
    pub project_registration_id: String,
    #[serde(rename(deserialize = "userid"))]
    pub user_id: String,
    #[serde(rename(deserialize = "projectid"))]
    pub project_id: String,
    #[serde(rename(deserialize = "activity"))]
    pub activity: String,
    #[serde(rename(deserialize = "phaseid"))]
    pub phase_id: String,
    #[serde(rename(deserialize = "planningtaskid"))]
    pub planning_task_id: Value,
    #[serde(rename(deserialize = "starttime"))]
    pub start_time: String,
    #[serde(rename(deserialize = "usernote", serialize = "note"))]
    pub user_note: String,
    #[serde(rename(deserialize = "ticketdata"))]
    pub ticket_data: Value,
    #[serde(rename(deserialize = "internalnote"))]
    pub internal_note: Value,
    #[serde(rename(deserialize = "typeof"))]
    pub type_of: Value,
    #[serde(rename(deserialize = "attendencelogid"))]
    pub attendance_log_id: String,
    #[serde(rename(deserialize = "variationid"))]
    pub variation_id: Value,
    #[serde(rename(deserialize = "projtimehh"))]
    pub proj_time_hh: Value,
    #[serde(rename(deserialize = "projtimemm"))]
    pub proj_time_mm: Value,
    #[serde(rename(deserialize = "difference"))]
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
    pub favorite_type: Option<i64>,
    #[serde(rename(deserialize = "projectnr"))]
    pub project_nr: Value,
    #[serde(rename(deserialize = "hours"))]
    pub hours: i64,
    #[serde(rename(deserialize = "seconds"))]
    pub seconds: i64,
    #[serde(rename(deserialize = "minutes"))]
    pub minutes: i64,
    #[serde(rename(deserialize = "projectregistration"))]
    pub project_registration: TimerProjectRegistration,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerRegistrationPayload {
    pub absencetype: Value,
    pub attestlevel: AttestLevel,
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
        input_time: Option<String>,
        proj_time: Option<String>,
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
            inputtime: input_time.unwrap_or("00:00".to_string()),
            phaseid: "Default".to_string(),
            projtime: proj_time.unwrap_or("00:00".to_string()),
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
    pub input_time: Option<String>,
    pub proj_time: Option<String>,
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
        input_time: Option<String>,
        proj_time: Option<String>,
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
            input_time,
            proj_time,
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
            start_timer_options.input_time,
            start_timer_options.proj_time,
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerPayload {
    #[serde(rename(serialize = "usernote"))]
    pub user_note: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerResponse {
    #[serde(rename(deserialize = "projectregistration"))]
    pub project_registration: SaveTimerProjectRegistration,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerProjectRegistration {
    #[serde(rename(deserialize = "absencetype"))]
    pub absence_type: Option<Value>,
    #[serde(rename(deserialize = "attestlevel"))]
    pub attest_level: Option<AttestLevel>,
    #[serde(rename(deserialize = "activityname"))]
    pub activity_name: String,
    #[serde(rename(deserialize = "requirenote"))]
    pub require_note: Option<Value>,
    #[serde(rename(deserialize = "userid"))]
    pub user_id: String,
    #[serde(rename(deserialize = "favoritetype"))]
    pub favorite_type: Option<Value>,
    #[serde(rename(deserialize = "regday"))]
    pub reg_day: String,
    #[serde(rename(deserialize = "projectid"))]
    pub project_id: String,
    #[serde(rename(deserialize = "projectname"))]
    pub project_name: String,
    pub activity: String,
    #[serde(rename(deserialize = "phaseid"))]
    pub phase_id: String,
    #[serde(rename(deserialize = "phasename"))]
    pub phase_name: String,
    #[serde(rename(deserialize = "planningtaskid"))]
    pub planning_task_id: Option<Value>,
    #[serde(rename(deserialize = "projectregistrationid"))]
    pub project_registration_id: String,
    #[serde(rename(deserialize = "projtimehh"))]
    pub proj_time_hh: f64,
    #[serde(rename(deserialize = "projtimemm"))]
    pub proj_time_mm: i64,
    #[serde(rename(deserialize = "variationid"))]
    pub variation_id: Option<Value>,
    #[serde(rename(deserialize = "billtimehh"))]
    pub bill_time_hh: Option<Value>,
    #[serde(rename(deserialize = "billtimemm"))]
    pub bill_time_mm: Option<Value>,
    #[serde(rename(deserialize = "projectnr"))]
    pub project_nr: Option<Value>,
    #[serde(rename(deserialize = "usernote"))]
    pub user_note: String,
    #[serde(rename(deserialize = "internalnote"))]
    pub internal_note: String,
    #[serde(rename(deserialize = "projplandescription"))]
    pub proj_plan_description: Option<Value>,
    #[serde(rename(deserialize = "reportnr"))]
    pub report_nr: Option<Value>,
    #[serde(rename(deserialize = "planningtaskname"))]
    pub planning_task_name: Option<Value>,
    #[serde(rename(deserialize = "planningtype"))]
    pub planning_type: i64,
    #[serde(rename(deserialize = "cancreatedrivelog"))]
    pub can_create_drive_log: bool,
    #[serde(rename(deserialize = "timedistributiontype"))]
    pub time_distribution_type: String,
    #[serde(rename(deserialize = "customernames"))]
    pub customer_names: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTimerPayload {
    #[serde(rename(serialize = "usernote"))]
    pub user_note: String,
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
    pub favorite_type: Option<i64>,
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
