use std::cmp;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{app_state::AppState, auth::AuthSession, repositories::TimerRepository};

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
    let time_info = milltime_client.fetch_time_info(date_filter).await?;

    Ok((jar, Json(time_info)))
}

#[derive(Debug, Deserialize)]
pub struct TimeEntriesQuery {
    from: String,
    to: String,
    unique: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedTimeEntry {
    #[serde(flatten)]
    time_entry: milltime::TimeEntry,
    #[serde(with = "time::serde::rfc3339::option")]
    start_time: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    end_time: Option<time::OffsetDateTime>,
}

#[instrument(name = "get_time_entries", skip(jar, app_state, auth_session))]
pub async fn get_time_entries(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Query(query): Query<TimeEntriesQuery>,
) -> CookieJarResult<Json<Vec<ExtendedTimeEntry>>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let date_filter: milltime::DateFilter = format!("{},{}", query.from, query.to)
        .parse()
        .map_err(|_| ErrorResponse {
            status: StatusCode::BAD_REQUEST,
            error: MilltimeError::DateParseError,
            message: "could not parse date range".to_string(),
        })?;

    let user_calendar = milltime_client.fetch_user_calendar(&date_filter).await?;

    let time_entries_iter = user_calendar
        .weeks
        .into_iter()
        .flat_map(|week| week.days)
        .filter(|day| day.date >= date_filter.from && day.date <= date_filter.to) // Milltime returns entire weeks, even if the range is in the middle of the week
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

    let timer_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let timer_history = timer_repo.get_timer_history(&user.id).await.map_err(|e| {
        tracing::error!("failed to get timer history: {:?}", e);
        ErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: MilltimeError::FetchError,
            message: "failed to fetch timer history from database".to_string(),
        }
    })?;

    let time_entries = time_entries
        .into_iter()
        .map(|time_entry| {
            let start_time = timer_history
                .iter()
                .find(|timer| timer.registration_id == Some(time_entry.registration_id.clone()))
                .map(|timer| timer.start_time);
            let end_time = timer_history
                .iter()
                .find(|timer| timer.registration_id == Some(time_entry.registration_id.clone()))
                .and_then(|timer| timer.end_time);
            ExtendedTimeEntry {
                time_entry,
                start_time,
                end_time,
            }
        })
        .collect();

    Ok((jar, Json(time_entries)))
}
