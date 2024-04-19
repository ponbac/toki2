use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "PascalCase"))]
pub struct TimePeriodInfo {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub flex_time_previous_period: Option<f64>,
    pub flex_time_period: f64,
    #[serde(rename(deserialize = "FlexTimeCurrent"))]
    pub flex_time_total: f64,
    #[serde(rename(deserialize = "ScheduledPeriodTime"))]
    pub scheduled_time_period: f64,
    #[serde(rename(deserialize = "WorkedPeriodTime"))]
    pub worked_time_period: f64,
    #[serde(rename(deserialize = "AbsencePeriodTime"))]
    pub absence_time_period: f64,
    #[serde(rename(deserialize = "WorkedPeriodWithAbsenceTime"))]
    pub worked_time_with_absence_period: f64,
    #[serde(rename(deserialize = "PeriodTimeLeft"))]
    pub time_left_period: f64,
}
