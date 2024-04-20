use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub ticket: Ticket,
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
        reg_day: String,
        week_number: i64,
    ) -> Self {
        Self {
            activity,
            activityname: activity_name,
            projectid: project_id,
            projectname: project_name,
            userid: user_id,
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
        reg_day: String,
        week_number: i64,
    ) -> Self {
        Self {
            activity,
            activity_name,
            project_id,
            project_name,
            user_id,
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
            start_timer_options.reg_day,
            start_timer_options.week_number,
        )
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {}
