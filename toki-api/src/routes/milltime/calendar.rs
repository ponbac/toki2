use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use tracing::instrument;

use crate::app_state::AppState;

use super::{CookieJarResult, MilltimeCookieJarExt};

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
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "could not parse date range".to_string(),
            )
        })?;
    let time_info = milltime_client
        .fetch_time_info(date_filter)
        .await
        .map_err(|_| (StatusCode::OK, "".to_string()))?;

    Ok((jar, Json(time_info)))
}
