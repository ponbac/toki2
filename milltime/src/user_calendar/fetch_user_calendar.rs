use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Client as MilltimeClient, Credentials, MilltimeFetchError, MilltimeRowResponse, MilltimeURL,
};

pub async fn fetch_user_calendar(
    milltime_credentials: Credentials,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<UserCalendar, MilltimeFetchError> {
    let milltime_client = MilltimeClient::new(milltime_credentials);

    let url = MilltimeURL::new()
        .append_path("data/store/UserCalendar")
        .with_date_filter(&from, &to);

    let response: MilltimeRowResponse<RawUserCalendar> = milltime_client.fetch(url).await?;
    let raw_result = response.only_row()?;

    let transformed_weeks = raw_result.weeks.into_iter().map(Week::from).collect();

    Ok(UserCalendar {
        weeks: transformed_weeks,
    })
}

// Custom types
#[derive(Debug, Serialize)]
pub struct UserCalendar {
    pub weeks: Vec<Week>,
}

#[derive(Debug, Serialize)]
pub struct Week {
    pub weeknr: i64,
    pub total_hours: f64,
    pub worked_hours: f64,
    pub days: Vec<Day>,
}

impl From<RawWeek> for Week {
    fn from(raw_week: RawWeek) -> Self {
        let total_hours: f64 = raw_week
            .days
            .iter()
            .fold(0.0, |acc, raw_day| acc + raw_day.stdtime.unwrap_or(0.0));
        let worked_hours: f64 = raw_week.days.iter().fold(0.0, |acc, raw_day| {
            acc + raw_day
                .projectregistrations
                .iter()
                .fold(0.0, |acc, raw_project_registration| {
                    acc + raw_project_registration.projtimehh
                        + (raw_project_registration.projtimemm as f64 / 60.0)
                })
        });

        let days = raw_week.days.into_iter().map(Day::from).collect();

        Week {
            weeknr: raw_week.weeknr,
            total_hours,
            worked_hours,
            days,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub total_hours: f64,
    pub worked_hours: f64,
    pub time_entries: Vec<TimeEntry>,
}

impl From<RawDay> for Day {
    fn from(raw_day: RawDay) -> Self {
        let date =
            NaiveDate::parse_from_str(&raw_day.regday, "%Y-%m-%d").unwrap_or(NaiveDate::default());

        let total_hours = raw_day.stdtime.unwrap_or(0.0);
        let worked_hours =
            raw_day
                .projectregistrations
                .iter()
                .fold(0.0, |acc, raw_project_registration| {
                    acc + raw_project_registration.projtimehh
                        + (raw_project_registration.projtimemm as f64 / 60.0)
                });

        let time_entries = raw_day
            .projectregistrations
            .into_iter()
            .map(TimeEntry::from)
            .collect();

        Day {
            date,
            total_hours,
            worked_hours,
            time_entries,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TimeEntry {
    pub project_name: String,
    pub activity_name: String,
    pub hours: f64,
    pub note: String,
}

impl From<RawProjectregistration> for TimeEntry {
    fn from(raw_project_registration: RawProjectregistration) -> Self {
        TimeEntry {
            project_name: raw_project_registration.projectname,
            activity_name: raw_project_registration.activityname,
            hours: raw_project_registration.projtimehh
                + (raw_project_registration.projtimemm as f64 / 60.0),
            note: raw_project_registration.usernote,
        }
    }
}

// Raw types, these are the types that are returned from the Milltime API
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUserCalendar {
    pub previous_attest_level: i64,
    pub attest_level: i64,
    pub month: i64,
    pub weeks: Vec<RawWeek>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWeek {
    pub weeknr: i64,
    pub attestlevel: i64,
    pub days: Vec<RawDay>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDay {
    pub regday: String,
    pub regweek: String,
    pub week: i64,
    pub stdtime: Option<f64>,
    pub holiday: bool,
    pub monthday: i64,
    pub month: i64,
    pub attestlevel: i64,
    pub weeklyattestlevel: i64,
    pub projectregistrations: Vec<RawProjectregistration>,
    pub flexdiff: RawFlexdiff,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProjectregistration {
    pub attestlevel: Option<i64>,
    pub activityname: String,
    pub userid: String,
    pub favoritetype: Value,
    pub regday: String,
    pub projectid: String,
    pub projectname: String,
    pub activity: String,
    pub projectregistrationid: String,
    pub projtimehh: f64,
    pub projtimemm: i64,
    pub usernote: String,
    pub customernames: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFlexdiff {
    pub hh: Option<f64>,
    pub mm: Option<i64>,
}
