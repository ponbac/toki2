use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use chrono::Datelike;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    app_state::AppState,
    auth::AuthSession,
    repositories::{DatabaseTimer, TimerRepository},
};

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
    week_number: u32,
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
        .flat_map(|day| day.time_entries);

    // get timer history from database
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

    let db_timer_registrations: HashMap<String, DatabaseTimer> = timer_history
        .clone()
        .into_iter()
        .filter_map(|timer| {
            timer
                .registration_id
                .as_ref()
                .map(|reg_id| (reg_id.clone(), timer.clone()))
        })
        .collect();

    // merge database timer history with milltime time entries
    let time_entries_iter = time_entries_iter
        .map(|time_entry| {
            let start_time = db_timer_registrations
                .get(&time_entry.registration_id)
                .map(|timer| timer.start_time);
            let end_time = db_timer_registrations
                .get(&time_entry.registration_id)
                .and_then(|timer| timer.end_time);
            ExtendedTimeEntry {
                time_entry: time_entry.clone(),
                start_time,
                end_time,
                week_number: time_entry.date.iso_week().week(),
            }
        })
        .sorted_by(|a, b| {
            let date_cmp = b.time_entry.date.cmp(&a.time_entry.date);

            // if dates are equal, then compare by start_time
            if date_cmp == std::cmp::Ordering::Equal {
                b.start_time.cmp(&a.start_time)
            } else {
                date_cmp
            }
        });

    let time_entries = if query.unique.unwrap_or(false) {
        time_entries_iter
            .unique_by(|time_entry| {
                format!(
                    "{}-{}-{}",
                    time_entry.time_entry.project_name,
                    time_entry.time_entry.activity_name,
                    time_entry
                        .time_entry
                        .note
                        .as_ref()
                        .unwrap_or(&"".to_string())
                )
            })
            .collect()
    } else {
        time_entries_iter.collect()
    };

    Ok((jar, Json(time_entries)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditProjectRegistrationPayload {
    project_registration_id: String,
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    start_time: String,
    end_time: String,
    reg_day: String,
    week_number: i32,
    user_note: String,
}

#[instrument(name = "edit_project_registration", skip(jar, app_state))]
pub async fn edit_project_registration(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Json(payload): Json<EditProjectRegistrationPayload>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let start_time = time::OffsetDateTime::parse(
        &payload.start_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid start time format".to_string(),
    })?;
    let end_time = time::OffsetDateTime::parse(
        &payload.end_time,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ErrorResponse {
        status: StatusCode::BAD_REQUEST,
        error: MilltimeError::DateParseError,
        message: "Invalid end time format".to_string(),
    })?;

    let total_time = format!(
        "{:02}:{:02}",
        (end_time - start_time).whole_hours(),
        (end_time - start_time).whole_minutes() % 60
    );
    let mt_payload = milltime::ProjectRegistrationEditPayload::new(
        payload.project_registration_id,
        milltime_client.user_id().to_string(),
        payload.project_id,
        payload.project_name,
        payload.activity_id,
        payload.activity_name,
        total_time,
        payload.reg_day,
        payload.week_number,
        payload.user_note,
    );

    milltime_client
        .edit_project_registration(&mt_payload)
        .await?;

    // update end time of timer registration
    let timer_repo = app_state.milltime_repo;
    let timer = timer_repo
        .get_by_registration_id(&mt_payload.project_registration_id)
        .await?;

    if timer.is_some() {
        timer_repo
            .update_start_and_end_time(&mt_payload.project_registration_id, &start_time, &end_time)
            .await?;
    }

    Ok((jar, StatusCode::OK))
}
