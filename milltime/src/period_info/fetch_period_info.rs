use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{
    Client as MilltimeClient, Credentials, MilltimeFetchError, MilltimeRowResponse, MilltimeURL,
};

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

pub async fn fetch_time_period_info(
    milltime_credentials: Credentials,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<TimePeriodInfo, MilltimeFetchError> {
    let milltime_client = MilltimeClient::new(milltime_credentials);

    let url = MilltimeURL::new()
        .append_path("/data/store/TimeInfo")
        .with_date_filter(&from, &to);

    let response: MilltimeRowResponse<TimePeriodInfo> = milltime_client.fetch(url).await?;
    let result_row = response.only_row()?;

    Ok(TimePeriodInfo {
        from: Some(from),
        to: Some(to),
        ..result_row
    })
}
