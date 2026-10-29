//! Admin-only AI adoption, under `/ai-usage/admin`: how much AI each person
//! and the company use.
//!
//! Every route needs admin power: an admin in a browser session, never an API
//! token (`auth::require_admin_power`), and none is on the agent surface
//! (OpenAPI and MCP). Responses are at most as fine as one local day per
//! person and tool; they never carry hours or sessions. Code completions are
//! left out.
//!
//! `days` are each person's use of a tool on a local day: tokens and records
//! from their machines, and `costUsd`, the API-equivalent estimate of the
//! priced records, or from `credits.from` on, for usage the metered payer pays
//! for, the payer's own credits charged to the person that UTC day. While
//! `unpricedRecords` is positive, `costUsd` is a known subtotal. `companyDays`
//! are credits no person carries: accounts nobody owns that day and usage
//! the payer bills per repository. Credits after `credits.completeThrough`
//! may still grow.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    middleware,
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::Date;

use crate::{
    adapters::inbound::http::{ai_subscriptions::iso_date, ai_usage::AiUsageProvider},
    auth::require_admin_power,
    domain::{
        models::{
            AiAdoption, AiAdoptionCompanyDay, AiAdoptionCreditStatus, AiAdoptionDay,
            AiAdoptionModel, AiAdoptionPerson, AiAdoptionSeat, AiBillingUsage, AiUsageDateRange,
            UserId, BILLING_CURRENCY,
        },
        ports::inbound::AiAdoptionService,
    },
    routes::ApiError,
};

type QueryParams<T> = WithRejection<Query<T>, ApiError>;
type PathParam<T> = WithRejection<Path<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiAdoptionService>: FromRef<S>,
{
    Router::new()
        .route("/adoption", get(adoption))
        .route("/adoption/developers/{user_id}/models", get(models))
        .route_layer(middleware::from_fn(require_admin_power))
}

/// A range of local days, both inclusive; either end may be left out.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeQuery {
    #[serde(default, with = "iso_date::option")]
    from: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    to: Option<Date>,
}

async fn adoption(
    State(service): State<Arc<dyn AiAdoptionService>>,
    WithRejection(Query(query), _): QueryParams<RangeQuery>,
) -> Result<Json<AdoptionResponse>, ApiError> {
    let adoption = service.adoption(query.from, query.to).await?;
    Ok(Json(AdoptionResponse::from(adoption)))
}

async fn models(
    State(service): State<Arc<dyn AiAdoptionService>>,
    WithRejection(Path(user_id), _): PathParam<i32>,
    WithRejection(Query(query), _): QueryParams<RangeQuery>,
) -> Result<Json<ModelsResponse>, ApiError> {
    let (dates, models) = service
        .models(&UserId::new(user_id), query.from, query.to)
        .await?;
    Ok(Json(ModelsResponse {
        range: RangeResponse::from(dates),
        models: models.into_iter().map(ModelResponse::from).collect(),
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeResponse {
    #[serde(with = "iso_date")]
    from: Date,
    #[serde(with = "iso_date")]
    to: Date,
}

impl From<AiUsageDateRange> for RangeResponse {
    fn from(dates: AiUsageDateRange) -> Self {
        Self {
            from: dates.start(),
            to: dates.last_day(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdoptionResponse {
    /// Today in `time_zone`.
    #[serde(with = "iso_date")]
    today: Date,
    time_zone: String,
    #[serde(flatten)]
    range: RangeResponse,
    /// The currency the bill is in, for showing costs.
    currency: &'static str,
    credits: Option<CreditStatusResponse>,
    people: Vec<PersonResponse>,
    days: Vec<DayResponse>,
    company_days: Vec<CompanyDayResponse>,
}

impl From<AiAdoption> for AdoptionResponse {
    fn from(adoption: AiAdoption) -> Self {
        Self {
            today: adoption.today,
            time_zone: adoption.time_zone.as_str().to_string(),
            range: RangeResponse::from(adoption.dates),
            currency: BILLING_CURRENCY,
            credits: adoption.credits.map(CreditStatusResponse::from),
            people: adoption
                .people
                .into_iter()
                .map(PersonResponse::from)
                .collect(),
            days: adoption.days.into_iter().map(DayResponse::from).collect(),
            company_days: adoption
                .company_days
                .into_iter()
                .map(CompanyDayResponse::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreditStatusResponse {
    provider: AiUsageProvider,
    #[serde(with = "iso_date")]
    from: Date,
    #[serde(with = "iso_date::option")]
    complete_through: Option<Date>,
}

impl From<AiAdoptionCreditStatus> for CreditStatusResponse {
    fn from(status: AiAdoptionCreditStatus) -> Self {
        Self {
            provider: status.payer.into(),
            from: status.from,
            complete_through: status.complete_through,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonResponse {
    user_id: i32,
    full_name: String,
    linked: bool,
    #[serde(with = "iso_date::option")]
    first_synced_on: Option<Date>,
    seats: Vec<SeatResponse>,
}

impl From<AiAdoptionPerson> for PersonResponse {
    fn from(person: AiAdoptionPerson) -> Self {
        Self {
            user_id: person.user_id.as_i32(),
            full_name: person.full_name,
            linked: person.linked,
            first_synced_on: person.first_synced_on,
            seats: person.seats.into_iter().map(SeatResponse::from).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeatResponse {
    provider: AiUsageProvider,
    #[serde(with = "iso_date")]
    since: Date,
}

impl From<AiAdoptionSeat> for SeatResponse {
    fn from(seat: AiAdoptionSeat) -> Self {
        Self {
            provider: seat.provider.into(),
            since: seat.since,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayResponse {
    user_id: i32,
    #[serde(with = "iso_date")]
    day: Date,
    tool: AiUsageProvider,
    cost_usd: f64,
    unpriced_records: i64,
    records: i64,
    /// Every token read and written: input, cache reads and writes, output.
    tokens: i64,
}

impl From<AiAdoptionDay> for DayResponse {
    fn from(day: AiAdoptionDay) -> Self {
        Self {
            user_id: day.user_id.as_i32(),
            day: day.day,
            tool: day.tool.into(),
            cost_usd: day.usage.priced_cost.to_usd(),
            unpriced_records: day.usage.unpriced_records,
            records: day.usage.records,
            tokens: total_tokens(&day.usage),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompanyDayResponse {
    #[serde(with = "iso_date")]
    day: Date,
    tool: AiUsageProvider,
    cost_usd: f64,
}

impl From<AiAdoptionCompanyDay> for CompanyDayResponse {
    fn from(day: AiAdoptionCompanyDay) -> Self {
        Self {
            day: day.day,
            tool: day.tool.into(),
            cost_usd: day.amount.to_usd(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelsResponse {
    #[serde(flatten)]
    range: RangeResponse,
    models: Vec<ModelResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelResponse {
    tool: AiUsageProvider,
    model: String,
    records: i64,
    tokens: i64,
}

impl From<AiAdoptionModel> for ModelResponse {
    fn from(model: AiAdoptionModel) -> Self {
        Self {
            tool: model.tool.into(),
            model: model.model,
            records: model.usage.records,
            tokens: total_tokens(&model.usage),
        }
    }
}

/// Saturates rather than fails: a display total of the disjoint categories.
fn total_tokens(usage: &AiBillingUsage) -> i64 {
    let tokens = usage.tokens;
    tokens
        .input
        .saturating_add(tokens.cache_read)
        .saturating_add(tokens.cache_write)
        .saturating_add(tokens.output)
}
