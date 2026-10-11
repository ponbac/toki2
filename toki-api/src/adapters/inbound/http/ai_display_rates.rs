//! Rates for showing API-equivalent USD estimates in another currency, for the
//! personal usage view.
//!
//! The rates are the ones a month bills at. They reveal nobody's usage, so any
//! signed-in user reads them. They are for reading estimates on screen;
//! billing converts each line by its own rules.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Query, State},
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::Date;

use crate::{
    adapters::inbound::http::ai_subscriptions::iso_date,
    domain::{
        models::{AiDisplayRate, AiDisplayRates},
        ports::inbound::AiDisplayRateService,
    },
    routes::ApiError,
};

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiDisplayRateService>: FromRef<S>,
{
    Router::new().route("/display-rates", get(get_display_rates))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisplayRateParams {
    /// The last local day of the usage being shown, `YYYY-MM-DD`. Defaults to
    /// today; a later day counts as today.
    #[serde(default, with = "iso_date::option")]
    to: Option<Date>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayRatesResponse {
    /// The month whose rates these are, `YYYY-MM`: the month of `to`, or the
    /// month before while that month has no USD rate.
    month: String,
    /// The billing currency first. Empty when no rate is known: estimates can
    /// only be shown in USD.
    rates: Vec<DisplayRateResponse>,
    /// A fetch of the rates is still in progress; ask again shortly.
    pending: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayRateResponse {
    /// ISO 4217 code, such as `SEK`.
    currency: String,
    /// Units of `currency` per US dollar, such as `9.50843`.
    per_usd: String,
    /// An average of the days published so far, which later days change.
    provisional: bool,
    /// Built on a rate an admin set for the month's bill.
    overridden: bool,
}

impl From<AiDisplayRate> for DisplayRateResponse {
    fn from(rate: AiDisplayRate) -> Self {
        Self {
            currency: rate.currency.as_str().to_string(),
            per_usd: rate.per_usd.to_decimal(),
            provisional: rate.provisional,
            overridden: rate.overridden,
        }
    }
}

impl From<AiDisplayRates> for DisplayRatesResponse {
    fn from(rates: AiDisplayRates) -> Self {
        Self {
            month: rates.month.to_string(),
            rates: rates.rates.into_iter().map(Into::into).collect(),
            pending: rates.pending,
        }
    }
}

/// The rates to show a range of usage at: its last day's month's.
async fn get_display_rates(
    State(service): State<Arc<dyn AiDisplayRateService>>,
    WithRejection(Query(params), _): WithRejection<Query<DisplayRateParams>, ApiError>,
) -> Result<Json<DisplayRatesResponse>, ApiError> {
    let rates = service.display_rates(params.to).await?;

    Ok(Json(rates.into()))
}

#[cfg(test)]
mod tests;
