use std::cmp;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use itertools::Itertools;
use serde::Deserialize;
use tracing::instrument;

use crate::app_state::AppState;

use super::{CookieJarResult, ErrorResponse, MilltimeCookieJarExt, MilltimeError};

#[derive(Debug, Deserialize)]
pub struct DateFilterQuery {
    from: String,
    to: String,
}

#[instrument(name = "get_time_info", skip(jar, app_state))]
pub async fn get_time_info(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> CookieJarResult<Json<milltime::TimeInfo>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let date_filter: milltime::DateFilter = format!("{},{}", date_filter.from, date_filter.to)
        .parse()
        .map_err(|_| ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            error: MilltimeError::DateParseError,
            message: "could not parse date range".to_string(),
        })?;
    let time_info = milltime_client
        .fetch_time_info(date_filter)
        .await
        .map_err(|_| ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: "failed to fetch time info from milltime".to_string(),
        })?;

    Ok((jar, Json(time_info)))
}

#[derive(Debug, Deserialize)]
pub struct TimeEntriesQuery {
    from: String,
    to: String,
    unique: Option<bool>,
}

#[instrument(name = "get_time_entries", skip(jar, app_state))]
pub async fn get_time_entries(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> CookieJarResult<Json<Vec<milltime::TimeEntry>>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let date_filter: milltime::DateFilter = format!("{},{}", query.from, query.to)
        .parse()
        .map_err(|_| ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            error: MilltimeError::DateParseError,
            message: "could not parse date range".to_string(),
        })?;

    let user_calendar = milltime_client
        .fetch_user_calendar(date_filter)
        .await
        .map_err(|_| ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: "failed to fetch user calendar from milltime".to_string(),
        })?;

    let time_entries_iter = user_calendar
        .weeks
        .into_iter()
        .flat_map(|week| week.days)
        .sorted_by_key(|day| cmp::Reverse(day.date))
        .flat_map(|day| day.time_entries);
    let time_entries: Vec<milltime::TimeEntry> = if query.unique.unwrap_or(false) {
        time_entries_iter
            .unique_by(|time_entry| {
                format!(
                    "{}-{}-{}",
                    time_entry.project_name,
                    time_entry.activity_name,
                    time_entry.note.as_ref().unwrap_or(&"".to_string())
                )
            })
            .collect()
    } else {
        time_entries_iter.collect()
    };

    Ok((jar, Json(time_entries)))
}
