//! AI subscriptions that developers declare, and plan hints that contradict them.
//!
//! Developers manage their own subscriptions. The admin routes reach every
//! user's and need admin power: an admin in a browser session, never an API
//! token (`AuthMethod::allows_admin_power`), the same rule as project mappings.
//!
//! Every malformed body, path or query is a `400` with the usual JSON error.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{get, put},
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::{IntoParams, ToSchema};

use crate::{
    adapters::inbound::http::{ai_usage::AiUsageProvider, ErrorResponse},
    auth::{require_admin_power, AuthUser},
    domain::{
        models::{
            AiCurrency, AiLocalCalendar, AiMonthlyCost, AiPlanMismatchRun, AiPlanMismatches,
            AiSubscription, AiSubscriptionId, AiSubscriptionPeriod, AiSubscriptionPlan,
            AiSubscriptionScope, AiSubscriptionTerms, UserId,
        },
        ports::inbound::AiSubscriptionService,
    },
    routes::ApiError,
};

/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;
/// A path parameter whose rejection is a `400` JSON error.
type PathParam<T> = WithRejection<Path<T>, ApiError>;
/// A query string whose rejection is a `400` JSON error.
type QueryParams<T> = WithRejection<Query<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiSubscriptionService>: FromRef<S>,
{
    // Admin power needs an admin in a browser session; a request that presents
    // an API token authenticates as the token, even with an admin's cookie.
    let admin = Router::new()
        .route(
            "/subscriptions",
            get(admin_list_subscriptions).post(admin_create_subscription),
        )
        .route(
            "/subscriptions/{subscription_id}",
            put(admin_update_subscription).delete(admin_delete_subscription),
        )
        .route(
            "/subscription-mismatches",
            get(admin_list_subscription_mismatches),
        )
        .route_layer(middleware::from_fn(require_admin_power));

    Router::new()
        .route(
            "/subscriptions",
            get(list_subscriptions).post(create_subscription),
        )
        .route(
            "/subscriptions/{subscription_id}",
            put(update_subscription).delete(delete_subscription),
        )
        .route(
            "/subscription-mismatches",
            get(list_subscription_mismatches),
        )
        .nest("/admin", admin)
}

/// A subscription's terms. Dates are calendar days in the server's AI usage
/// time zone, which list responses name.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionRequest {
    provider: AiUsageProvider,
    /// Free text, such as `Claude Max 5x`, `ChatGPT Pro` or `Copilot Business`.
    /// At most 64 Unicode code points.
    #[schema(min_length = 1, max_length = 64, example = "Claude Max 5x")]
    plan: String,
    /// The monthly fee as an exact decimal string with at most two decimals.
    #[schema(pattern = r"^[0-9]{1,10}(\.[0-9]{1,2})?$", example = "1100.00")]
    monthly_cost: String,
    /// ISO 4217 code of the fee's currency, such as `SEK` or `USD`.
    #[schema(pattern = "^[A-Za-z]{3}$", example = "SEK")]
    currency: String,
    /// The first covered day, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    valid_from: Date,
    /// The last covered day, inclusive. Null or absent while ongoing.
    #[serde(default, with = "iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    valid_to: Option<Date>,
}

impl AiSubscriptionRequest {
    fn into_terms(self) -> Result<AiSubscriptionTerms, ApiError> {
        Ok(AiSubscriptionTerms {
            provider: self.provider.into(),
            plan: AiSubscriptionPlan::parse(&self.plan)?,
            monthly_cost: AiMonthlyCost::parse(&self.monthly_cost)?,
            currency: AiCurrency::parse(&self.currency)?,
            period: AiSubscriptionPeriod::new(self.valid_from, self.valid_to)?,
        })
    }
}

/// A subscription declared for any user; admin only.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiSubscriptionRequest {
    user_id: i32,
    #[serde(flatten)]
    terms: AiSubscriptionRequest,
}

/// Subscriptions, with the calendar their dates belong to.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionListResponse {
    /// The IANA time zone whose calendar days subscription dates are, such as
    /// `Europe/Stockholm`.
    time_zone: String,
    /// Today's date in that time zone, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    today: Date,
    subscriptions: Vec<AiSubscriptionResponse>,
}

impl AiSubscriptionListResponse {
    fn new(calendar: AiLocalCalendar, subscriptions: Vec<AiSubscription>) -> Self {
        Self {
            time_zone: calendar.time_zone.as_str().to_string(),
            today: calendar.today,
            subscriptions: subscriptions.into_iter().map(Into::into).collect(),
        }
    }
}

/// A declared subscription. Days it covers bill its fee; other days bill that
/// provider's usage at its estimated API cost in billing reports.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionResponse {
    id: i32,
    user_id: i32,
    provider: AiUsageProvider,
    plan: String,
    /// The monthly fee as an exact decimal string with two decimals.
    #[schema(example = "1100.00")]
    monthly_cost: String,
    /// ISO 4217 code, such as `SEK`.
    currency: String,
    /// The first covered day, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    valid_from: Date,
    /// The last covered day, inclusive, or null while ongoing.
    #[serde(with = "iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    valid_to: Option<Date>,
}

impl From<AiSubscription> for AiSubscriptionResponse {
    fn from(subscription: AiSubscription) -> Self {
        let terms = subscription.terms;
        Self {
            id: subscription.id.as_i32(),
            user_id: subscription.user_id.as_i32(),
            provider: terms.provider.into(),
            plan: terms.plan.as_str().to_string(),
            monthly_cost: terms.monthly_cost.to_string(),
            currency: terms.currency.as_str().to_string(),
            valid_from: terms.period.valid_from(),
            valid_to: terms.period.valid_to(),
        }
    }
}

/// The mismatches found between two local days.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionMismatchListResponse {
    /// The first local day searched, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    from: Date,
    /// The last local day searched, inclusive.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    to: Date,
    mismatches: Vec<AiSubscriptionMismatchResponse>,
}

impl From<AiPlanMismatches> for AiSubscriptionMismatchListResponse {
    fn from(mismatches: AiPlanMismatches) -> Self {
        Self {
            from: mismatches.dates.start(),
            to: mismatches.dates.last_day(),
            mismatches: mismatches.runs.into_iter().map(Into::into).collect(),
        }
    }
}

/// Local days on which a provider reported a paid plan, such as Codex
/// `plan_type` `pro`, but no subscription to that provider is declared for them,
/// so their usage bills at its estimated API cost. The days lie between the
/// same declared subscriptions, so one declaration can cover them all. Hints
/// are evidence only.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionMismatchResponse {
    user_id: i32,
    provider: AiUsageProvider,
    /// The first mismatch day, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    first_day: Date,
    /// The last mismatch day, inclusive.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    last_day: Date,
    /// How many days from `firstDay` to `lastDay` have mismatches.
    days: u32,
    /// The paid plans the provider reported, such as `pro`.
    plans: Vec<String>,
    /// The last day before the provider's next declared subscription, or null
    /// when none follows. A subscription from `firstDay` through this day
    /// overlaps no other.
    #[serde(with = "iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    last_uncovered_day: Option<Date>,
}

impl From<AiPlanMismatchRun> for AiSubscriptionMismatchResponse {
    fn from(run: AiPlanMismatchRun) -> Self {
        Self {
            user_id: run.user_id.as_i32(),
            provider: run.provider.into(),
            first_day: run.first_day,
            last_day: run.last_day,
            days: run.days,
            plans: run.plans,
            last_uncovered_day: run.last_uncovered_day,
        }
    }
}

/// The local days to search for mismatches.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AiSubscriptionMismatchQuery {
    /// The first local day, `YYYY-MM-DD`. Defaults to 89 days before `to`, so
    /// the search covers 90 days.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    from: Option<Date>,
    /// The last local day, inclusive. Defaults to today in the server's AI usage
    /// time zone.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    to: Option<Date>,
}

/// An admin listing, optionally of one user.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminListQuery {
    user_id: Option<i32>,
}

/// An admin mismatch search, optionally of one user. Serde's `flatten` cannot
/// read numbers from a query string, so the fields are repeated.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminMismatchQuery {
    user_id: Option<i32>,
    #[serde(default, with = "iso_date::option")]
    from: Option<Date>,
    #[serde(default, with = "iso_date::option")]
    to: Option<Date>,
}

fn admin_scope(user_id: Option<i32>) -> AiSubscriptionScope {
    user_id.map_or(AiSubscriptionScope::AllUsers, |user_id| {
        AiSubscriptionScope::User(UserId::new(user_id))
    })
}

/// List your AI subscriptions
///
/// Lists the AI subscriptions you declared, by provider and latest start first,
/// with the time zone whose calendar days their dates are and today's date
/// there. A day that none of your subscriptions to a provider covers bills that
/// provider's usage at its estimated API cost in billing reports.
#[utoipa::path(
    get,
    path = "/ai-usage/subscriptions",
    operation_id = "listAiSubscriptions",
    tag = "AI usage",
    responses(
        (status = 200, description = "Your subscriptions", body = AiSubscriptionListResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Subscriptions could not be read", body = ErrorResponse)
    )
)]
pub async fn list_subscriptions(
    user: AuthUser,
    State(service): State<Arc<dyn AiSubscriptionService>>,
) -> Result<Json<AiSubscriptionListResponse>, ApiError> {
    list(&service, AiSubscriptionScope::User(user.id)).await
}

/// Declare an AI subscription
///
/// Declares a subscription of yours to a provider, such as Claude Max 5x from a
/// given day. Its monthly fee then pays for that provider's usage on every day
/// from `validFrom` through `validTo`, or on without end when `validTo` is null.
/// Periods of your subscriptions to one provider must not share a day: end the
/// current one before its successor starts.
#[utoipa::path(
    post,
    path = "/ai-usage/subscriptions",
    operation_id = "createAiSubscription",
    tag = "AI usage",
    request_body = AiSubscriptionRequest,
    responses(
        (status = 201, description = "Subscription declared", body = AiSubscriptionResponse),
        (status = 400, description = "The body is not a valid subscription", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 409, description = "The period overlaps another subscription to the provider", body = ErrorResponse),
        (status = 500, description = "The subscription could not be stored", body = ErrorResponse)
    )
)]
pub async fn create_subscription(
    user: AuthUser,
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Json(request), _): JsonBody<AiSubscriptionRequest>,
) -> Result<(StatusCode, Json<AiSubscriptionResponse>), ApiError> {
    create(&service, user.id, request).await
}

/// Update an AI subscription
///
/// Replaces every term of one of your subscriptions, for example to end it by
/// setting `validTo`. Periods of your subscriptions to one provider must not
/// share a day.
#[utoipa::path(
    put,
    path = "/ai-usage/subscriptions/{subscription_id}",
    operation_id = "updateAiSubscription",
    tag = "AI usage",
    params(("subscription_id" = i32, Path, description = "The subscription ID")),
    request_body = AiSubscriptionRequest,
    responses(
        (status = 200, description = "Subscription updated", body = AiSubscriptionResponse),
        (status = 400, description = "The ID or body is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "You have no subscription with this ID", body = ErrorResponse),
        (status = 409, description = "The period overlaps another subscription to the provider", body = ErrorResponse),
        (status = 500, description = "The subscription could not be stored", body = ErrorResponse)
    )
)]
pub async fn update_subscription(
    user: AuthUser,
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Path(subscription_id), _): PathParam<i32>,
    WithRejection(Json(request), _): JsonBody<AiSubscriptionRequest>,
) -> Result<Json<AiSubscriptionResponse>, ApiError> {
    update(
        &service,
        AiSubscriptionScope::User(user.id),
        subscription_id,
        request,
    )
    .await
}

/// Delete an AI subscription
///
/// Deletes one of your subscriptions, as if it had never been declared: its days
/// bill as API usage again. To record that a subscription ended, set its
/// `validTo` instead.
#[utoipa::path(
    delete,
    path = "/ai-usage/subscriptions/{subscription_id}",
    operation_id = "deleteAiSubscription",
    tag = "AI usage",
    params(("subscription_id" = i32, Path, description = "The subscription ID")),
    responses(
        (status = 204, description = "Subscription deleted"),
        (status = 400, description = "The ID is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 404, description = "You have no subscription with this ID", body = ErrorResponse),
        (status = 500, description = "The subscription could not be deleted", body = ErrorResponse)
    )
)]
pub async fn delete_subscription(
    user: AuthUser,
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Path(subscription_id), _): PathParam<i32>,
) -> Result<StatusCode, ApiError> {
    delete(
        &service,
        AiSubscriptionScope::User(user.id),
        subscription_id,
    )
    .await
}

/// List your subscription mismatches
///
/// Lists local days, between `from` and `to`, on which a provider reported a
/// paid plan for your usage, such as Codex `plan_type` `pro`, although no
/// subscription of yours to that provider is declared for them. Their usage
/// bills at its estimated API cost until you declare the subscription. Days
/// between the same declared subscriptions form one entry. Only Codex reports
/// plans today, a `free` plan is no evidence of a subscription, and a missing
/// plan proves nothing.
#[utoipa::path(
    get,
    path = "/ai-usage/subscription-mismatches",
    operation_id = "listAiSubscriptionMismatches",
    tag = "AI usage",
    params(AiSubscriptionMismatchQuery),
    responses(
        (status = 200, description = "Mismatches by provider and first day", body = AiSubscriptionMismatchListResponse),
        (status = 400, description = "`from` or `to` is not a date, or `from` is after `to`", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Mismatches could not be read", body = ErrorResponse)
    )
)]
pub async fn list_subscription_mismatches(
    user: AuthUser,
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Query(query), _): QueryParams<AiSubscriptionMismatchQuery>,
) -> Result<Json<AiSubscriptionMismatchListResponse>, ApiError> {
    mismatches(
        &service,
        AiSubscriptionScope::User(user.id),
        query.from,
        query.to,
    )
    .await
}

async fn admin_list_subscriptions(
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Query(query), _): QueryParams<AdminListQuery>,
) -> Result<Json<AiSubscriptionListResponse>, ApiError> {
    list(&service, admin_scope(query.user_id)).await
}

async fn admin_create_subscription(
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Json(request), _): JsonBody<AdminAiSubscriptionRequest>,
) -> Result<(StatusCode, Json<AiSubscriptionResponse>), ApiError> {
    create(&service, UserId::new(request.user_id), request.terms).await
}

async fn admin_update_subscription(
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Path(subscription_id), _): PathParam<i32>,
    WithRejection(Json(request), _): JsonBody<AiSubscriptionRequest>,
) -> Result<Json<AiSubscriptionResponse>, ApiError> {
    update(
        &service,
        AiSubscriptionScope::AllUsers,
        subscription_id,
        request,
    )
    .await
}

async fn admin_delete_subscription(
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Path(subscription_id), _): PathParam<i32>,
) -> Result<StatusCode, ApiError> {
    delete(&service, AiSubscriptionScope::AllUsers, subscription_id).await
}

async fn admin_list_subscription_mismatches(
    State(service): State<Arc<dyn AiSubscriptionService>>,
    WithRejection(Query(query), _): QueryParams<AdminMismatchQuery>,
) -> Result<Json<AiSubscriptionMismatchListResponse>, ApiError> {
    mismatches(&service, admin_scope(query.user_id), query.from, query.to).await
}

async fn list(
    service: &Arc<dyn AiSubscriptionService>,
    scope: AiSubscriptionScope,
) -> Result<Json<AiSubscriptionListResponse>, ApiError> {
    let subscriptions = service.list(scope).await?;
    let calendar = service.calendar().await?;
    Ok(Json(AiSubscriptionListResponse::new(
        calendar,
        subscriptions,
    )))
}

async fn create(
    service: &Arc<dyn AiSubscriptionService>,
    user_id: UserId,
    request: AiSubscriptionRequest,
) -> Result<(StatusCode, Json<AiSubscriptionResponse>), ApiError> {
    let subscription = service.create(&user_id, &request.into_terms()?).await?;
    Ok((StatusCode::CREATED, Json(subscription.into())))
}

async fn update(
    service: &Arc<dyn AiSubscriptionService>,
    scope: AiSubscriptionScope,
    subscription_id: i32,
    request: AiSubscriptionRequest,
) -> Result<Json<AiSubscriptionResponse>, ApiError> {
    let subscription = service
        .update(
            scope,
            AiSubscriptionId::new(subscription_id),
            &request.into_terms()?,
        )
        .await?;
    Ok(Json(subscription.into()))
}

async fn delete(
    service: &Arc<dyn AiSubscriptionService>,
    scope: AiSubscriptionScope,
    subscription_id: i32,
) -> Result<StatusCode, ApiError> {
    service
        .delete(scope, AiSubscriptionId::new(subscription_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mismatches(
    service: &Arc<dyn AiSubscriptionService>,
    scope: AiSubscriptionScope,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<Json<AiSubscriptionMismatchListResponse>, ApiError> {
    let mismatches = service.plan_mismatches(scope, from, to).await?;
    Ok(Json(mismatches.into()))
}

/// Local calendar dates as `YYYY-MM-DD`.
mod iso_date {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};
    use time::{format_description::BorrowedFormatItem, macros::format_description, Date};

    const FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[year]-[month]-[day]");

    pub fn serialize<S: Serializer>(date: &Date, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format(date)?)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        parse(&String::deserialize(deserializer)?)
    }

    /// Null for `None`. Absent keys need `#[serde(default)]`.
    pub mod option {
        use super::*;

        pub fn serialize<S: Serializer>(
            date: &Option<Date>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match date {
                Some(date) => serializer.serialize_some(&format(date)?),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<Date>, D::Error> {
            Option::<String>::deserialize(deserializer)?
                .map(|text| parse(&text))
                .transpose()
        }
    }

    fn format<E: serde::ser::Error>(date: &Date) -> Result<String, E> {
        date.format(FORMAT).map_err(E::custom)
    }

    fn parse<E: Error>(text: &str) -> Result<Date, E> {
        Date::parse(text, FORMAT)
            .ok()
            .filter(|date| text.len() == 10 && date.year() >= 1)
            .ok_or_else(|| {
                E::custom(format!(
                    "expected a date as YYYY-MM-DD from year 0001, got {text:?}"
                ))
            })
    }
}

#[cfg(test)]
mod tests;
