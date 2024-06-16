use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct TimeInfo {
    #[serde(rename(deserialize = "Overtimes"))]
    pub overtimes: Vec<Overtime>,
    #[serde(rename(deserialize = "FlexTimePreviousPeriod"))]
    pub flex_time_previous_period: Option<f64>,
    #[serde(rename(deserialize = "FlexTimePeriod"))]
    pub flex_time_period: f64,
    #[serde(rename(deserialize = "FlexTimeCurrent"))]
    pub flex_time_current: f64,
    #[serde(rename(deserialize = "FlexWithdrawal"))]
    pub flex_withdrawal: f64,
    #[serde(rename(deserialize = "ScheduledPeriodTime"))]
    pub scheduled_period_time: f64,
    #[serde(rename(deserialize = "WorkedPeriodTime"))]
    pub worked_period_time: f64,
    #[serde(rename(deserialize = "AbsencePeriodTime"))]
    pub absence_period_time: f64,
    #[serde(rename(deserialize = "WorkedPeriodWithAbsenceTime"))]
    pub worked_period_with_absence_time: f64,
    #[serde(rename(deserialize = "PeriodTimeLeft"))]
    pub period_time_left: f64,
    #[serde(rename(deserialize = "MTInfoDetailRow"))]
    pub mtinfo_detail_row: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overtime {
    pub key: String,
    pub value: f64,
    pub label: String,
}
