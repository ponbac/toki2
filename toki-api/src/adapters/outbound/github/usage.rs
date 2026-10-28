//! GitHub's billing usage of the organization, as a `MeteredUsageSource`:
//!
//! - `GET /organizations/{org}/settings/billing/ai_credit/usage?year&month[&day][&user]`:
//!   credits per product, SKU and model, of one user or (without `user`) the
//!   whole organization, for a month or a day.
//! - `GET /organizations/{org}/settings/billing/usage?year&month[&day]`: the
//!   organization's rows per SKU and repository, each dated; the month form
//!   returns every day's rows in one call.
//! - `GET /organizations/{org}/settings/billing/usage/summary?year&month&product=copilot`:
//!   the month's seats and credits with the pool discount.
//!
//! Numbers are read from the JSON text exactly (`RawValue`), exponents
//! included, never through floating point. Every response is checked against
//! its request: the period, the user, the organization and each row's date; a
//! mismatch, a negative or non-numeric amount, or rows that cannot be merged
//! is a `Response` error, so a slice never stores something else.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::value::RawValue;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{client::GithubError, labels, GithubApp};
use crate::domain::{
    models::{
        merge_credits, merge_org_charges, utc_day, AiBillingMonth, AiDecimal, AiMeteredAmounts,
        AiMeteredCredit, AiMeteredOrgCharge, AiMeteredPeriod, AiMeteredScope, AiMeteredSummaryLine,
    },
    ports::outbound::MeteredUsageSource,
    MeteredUsageSourceError,
};

/// The longest stored product, SKU, label or repository, in characters.
const MAX_NAME_CHARS: usize = 200;
/// The longest stored unit, in characters.
const MAX_UNIT_CHARS: usize = 100;

/// Copilot's billed usage in a GitHub organization.
pub struct GithubMeteredUsageSource {
    app: GithubApp,
}

impl GithubMeteredUsageSource {
    pub fn new(app: GithubApp) -> Self {
        Self { app }
    }

    fn organization(&self) -> &str {
        &self.app.organization
    }

    /// GitHub's answers in the usage source's terms, naming the scope.
    fn usage_error(&self, error: GithubError, user: Option<&str>) -> MeteredUsageSourceError {
        let organization = self.organization();
        match error {
            GithubError::Forbidden(message) => MeteredUsageSourceError::Forbidden(format!(
                "GitHub refused {organization}'s billing usage ({message}); the app needs the \
                 organization permission Administration: read"
            )),
            GithubError::NotFound(_) => MeteredUsageSourceError::NotFound(match user {
                Some(login) => format!("GitHub knows no Copilot usage for {login}"),
                None => format!("{organization} not found, or the app is not installed on it"),
            }),
            other => other.into(),
        }
    }

    async fn get(&self, path: &str, user: Option<&str>) -> Result<String, MeteredUsageSourceError> {
        self.app
            .client
            .get(path)
            .await
            .map(super::client::GithubResponse::into_body)
            .map_err(|error| self.usage_error(error, user))
    }
}

/// The query of a period: `year`, `month` and, for a day, `day`.
fn period_query(period: AiMeteredPeriod) -> url::form_urlencoded::Serializer<'static, String> {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    let (year, month, day) = period_parts(period);
    query
        .append_pair("year", &year.to_string())
        .append_pair("month", &month.to_string());
    if let Some(day) = day {
        query.append_pair("day", &day.to_string());
    }
    query
}

fn period_parts(period: AiMeteredPeriod) -> (i32, u8, Option<u8>) {
    match period {
        AiMeteredPeriod::Month(month) => {
            let first = month.first_day();
            (first.year(), u8::from(first.month()), None)
        }
        AiMeteredPeriod::Day(day) => (day.year(), u8::from(day.month()), Some(day.day())),
    }
}

/// `GET .../ai_credit/usage` for a period, of a user or the organization.
pub(super) fn credits_path(
    organization: &str,
    period: AiMeteredPeriod,
    user: Option<&str>,
) -> String {
    let mut query = period_query(period);
    if let Some(user) = user {
        query.append_pair("user", user);
    }
    format!(
        "/organizations/{organization}/settings/billing/ai_credit/usage?{}",
        query.finish()
    )
}

/// `GET .../usage` for a period.
pub(super) fn usage_path(organization: &str, period: AiMeteredPeriod) -> String {
    format!(
        "/organizations/{organization}/settings/billing/usage?{}",
        period_query(period).finish()
    )
}

/// `GET .../usage/summary` of Copilot for a month.
pub(super) fn summary_path(organization: &str, month: AiBillingMonth) -> String {
    let mut query = period_query(AiMeteredPeriod::Month(month));
    query.append_pair("product", "copilot");
    format!(
        "/organizations/{organization}/settings/billing/usage/summary?{}",
        query.finish()
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimePeriod {
    year: i32,
    month: u8,
    #[serde(default)]
    day: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditsBody {
    time_period: TimePeriod,
    #[serde(default)]
    user: Option<String>,
    organization: String,
    usage_items: Vec<BillingItem>,
}

/// A line of a credit or summary response; summary lines name no model.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingItem {
    product: String,
    sku: String,
    #[serde(default)]
    model: Option<String>,
    unit_type: String,
    price_per_unit: Box<RawValue>,
    gross_quantity: Box<RawValue>,
    gross_amount: Box<RawValue>,
    discount_quantity: Box<RawValue>,
    discount_amount: Box<RawValue>,
    net_quantity: Box<RawValue>,
    net_amount: Box<RawValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageBody {
    usage_items: Vec<UsageItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageItem {
    date: String,
    product: String,
    sku: String,
    quantity: Box<RawValue>,
    unit_type: String,
    price_per_unit: Box<RawValue>,
    gross_amount: Box<RawValue>,
    discount_amount: Box<RawValue>,
    net_amount: Box<RawValue>,
    organization_name: String,
    #[serde(default)]
    repository_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryBody {
    time_period: TimePeriod,
    organization: String,
    #[serde(default)]
    product: Option<String>,
    usage_items: Vec<BillingItem>,
}

impl BillingItem {
    /// The line's amounts, each parsed exactly.
    fn amounts(&self) -> Result<AiMeteredAmounts, GithubError> {
        Ok(AiMeteredAmounts {
            unit_price: decimal("pricePerUnit", &self.price_per_unit)?,
            gross_quantity: decimal("grossQuantity", &self.gross_quantity)?,
            gross_amount: decimal("grossAmount", &self.gross_amount)?,
            discount_quantity: decimal("discountQuantity", &self.discount_quantity)?,
            discount_amount: decimal("discountAmount", &self.discount_amount)?,
            net_quantity: decimal("netQuantity", &self.net_quantity)?,
            net_amount: decimal("netAmount", &self.net_amount)?,
        })
    }
}

fn invalid(message: impl Into<String>) -> GithubError {
    GithubError::Response(message.into())
}

fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, GithubError> {
    serde_json::from_str(body)
        .map_err(|error| invalid(format!("unreadable GitHub billing response: {error}")))
}

/// A bare non-negative JSON number literal, parsed exactly. Strings, nulls
/// and negative numbers are refused, naming the field.
fn decimal(field: &str, raw: &RawValue) -> Result<AiDecimal, GithubError> {
    let text = raw.get();
    if text.starts_with(|ch: char| ch.is_ascii_digit()) {
        if let Some(value) = AiDecimal::parse(text) {
            return Ok(value);
        }
    }
    let shown: String = text.chars().take(40).collect();
    Err(invalid(format!(
        "{field} is not a non-negative number: {shown}"
    )))
}

/// `text` if it has at most `max` characters.
fn name(field: &str, text: String, max: usize) -> Result<String, GithubError> {
    if text.chars().count() > max {
        return Err(invalid(format!("{field} is longer than {max} characters")));
    }
    Ok(text)
}

fn check_period(time_period: &TimePeriod, period: AiMeteredPeriod) -> Result<(), GithubError> {
    let (year, month, day) = period_parts(period);
    if time_period.year != year || time_period.month != month || time_period.day != day {
        return Err(invalid(format!(
            "timePeriod {}-{}{} does not match the requested {year}-{month}{}",
            time_period.year,
            time_period.month,
            time_period
                .day
                .map(|day| format!("-{day}"))
                .unwrap_or_default(),
            day.map(|day| format!("-{day}")).unwrap_or_default(),
        )));
    }
    Ok(())
}

fn check_organization(field: &str, answered: &str, organization: &str) -> Result<(), GithubError> {
    if !answered.trim().eq_ignore_ascii_case(organization) {
        return Err(invalid(format!(
            "{field} {answered:?} is not the configured organization"
        )));
    }
    Ok(())
}

/// Credits of `scope` for `period` from an `ai_credit/usage` body.
pub(super) fn parse_credits(
    body: &str,
    organization: &str,
    scope: AiMeteredScope<'_>,
    period: AiMeteredPeriod,
) -> Result<Vec<AiMeteredCredit>, GithubError> {
    let body: CreditsBody = parse(body)?;
    check_period(&body.time_period, period)?;
    check_organization("organization", &body.organization, organization)?;
    let answered_user = body
        .user
        .as_deref()
        .map(str::trim)
        .filter(|user| !user.is_empty());
    match (scope, answered_user) {
        (AiMeteredScope::Account(login), Some(user))
            if user.eq_ignore_ascii_case(login.as_str()) => {}
        (AiMeteredScope::Organization, None) => {}
        (AiMeteredScope::Account(login), answered) => {
            return Err(invalid(format!(
                "GitHub answered usage of {} instead of {login}",
                answered.unwrap_or("the organization")
            )))
        }
        (AiMeteredScope::Organization, Some(user)) => {
            return Err(invalid(format!(
                "GitHub answered usage of {user} instead of the organization"
            )))
        }
    }

    let rows = body
        .usage_items
        .into_iter()
        .map(|item| {
            let amounts = item.amounts()?;
            let label = name("model", item.model.unwrap_or_default(), MAX_NAME_CHARS)?;
            let (model, label_kind) = labels::model(&label);
            if model.model.chars().count() > MAX_NAME_CHARS {
                return Err(invalid("model is too long"));
            }
            let kind = labels::credit_kind(
                &item.sku,
                &item.unit_type,
                &amounts.unit_price,
                label_kind,
                &model.model,
            );
            let credit = AiMeteredCredit {
                product: name("product", item.product, MAX_NAME_CHARS)?,
                sku: name("sku", item.sku, MAX_NAME_CHARS)?,
                unit: name("unitType", item.unit_type, MAX_UNIT_CHARS)?,
                model,
                kind,
                amounts,
            };
            if credit.gross_usd().is_none() {
                return Err(invalid("grossQuantity is out of range"));
            }
            Ok(credit)
        })
        .collect::<Result<Vec<_>, GithubError>>()?;
    merge_credits(rows).map_err(invalid)
}

/// The organization's rows for `period` from a `usage` body.
pub(super) fn parse_usage(
    body: &str,
    organization: &str,
    period: AiMeteredPeriod,
) -> Result<Vec<AiMeteredOrgCharge>, GithubError> {
    let body: UsageBody = parse(body)?;
    let rows = body
        .usage_items
        .into_iter()
        .map(|item| {
            check_organization("organizationName", &item.organization_name, organization)?;
            let reported_at = OffsetDateTime::parse(&item.date, &Rfc3339)
                .map_err(|_| invalid(format!("date {:?} is not an RFC 3339 instant", item.date)))?;
            let day = utc_day(reported_at);
            let within = match period {
                AiMeteredPeriod::Month(month) => month.contains(day),
                AiMeteredPeriod::Day(requested) => day == requested,
            };
            if !within {
                return Err(invalid(format!(
                    "a usage row dated {} lies outside the requested period",
                    item.date
                )));
            }
            let unit_price = decimal("pricePerUnit", &item.price_per_unit)?;
            let repository = name(
                "repositoryName",
                item.repository_name.unwrap_or_default(),
                MAX_NAME_CHARS,
            )?;
            let kind = labels::org_charge_kind(
                &item.product,
                &item.sku,
                &item.unit_type,
                &unit_price,
                &repository,
            );
            let charge = AiMeteredOrgCharge {
                day,
                product: name("product", item.product, MAX_NAME_CHARS)?,
                sku: name("sku", item.sku, MAX_NAME_CHARS)?,
                unit: name("unitType", item.unit_type, MAX_UNIT_CHARS)?,
                repository,
                kind,
                unit_price,
                quantity: decimal("quantity", &item.quantity)?,
                gross_amount: decimal("grossAmount", &item.gross_amount)?,
                discount_amount: decimal("discountAmount", &item.discount_amount)?,
                net_amount: decimal("netAmount", &item.net_amount)?,
                reported_at: Some(reported_at),
            };
            if kind.is_credits() && charge.credit_usd().is_none() {
                return Err(invalid("quantity is out of range"));
            }
            Ok(charge)
        })
        .collect::<Result<Vec<_>, GithubError>>()?;
    merge_org_charges(rows).map_err(invalid)
}

/// The month's summary lines from a `usage/summary` body.
pub(super) fn parse_summary(
    body: &str,
    organization: &str,
    month: AiBillingMonth,
) -> Result<Vec<AiMeteredSummaryLine>, GithubError> {
    let body: SummaryBody = parse(body)?;
    check_period(&body.time_period, AiMeteredPeriod::Month(month))?;
    check_organization("organization", &body.organization, organization)?;
    if let Some(product) = body.product.as_deref() {
        if !product.trim().eq_ignore_ascii_case("copilot") {
            return Err(invalid(format!(
                "the summary is of {product:?}, not copilot"
            )));
        }
    }
    let mut lines: Vec<AiMeteredSummaryLine> = Vec::new();
    for item in body.usage_items {
        let line = AiMeteredSummaryLine {
            kind: labels::summary_kind(&item.sku),
            amounts: item.amounts()?,
            product: name("product", item.product, MAX_NAME_CHARS)?,
            sku: name("sku", item.sku, MAX_NAME_CHARS)?,
            unit: name("unitType", item.unit_type, MAX_UNIT_CHARS)?,
        };
        let duplicate = lines.iter().any(|other| {
            (&other.product, &other.sku, &other.unit) == (&line.product, &line.sku, &line.unit)
        });
        if duplicate {
            return Err(invalid(format!(
                "the summary lists {} / {} twice",
                line.product, line.sku
            )));
        }
        lines.push(line);
    }
    Ok(lines)
}

#[async_trait]
impl MeteredUsageSource for GithubMeteredUsageSource {
    async fn credits(
        &self,
        scope: AiMeteredScope<'_>,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredCredit>, MeteredUsageSourceError> {
        let user = match scope {
            AiMeteredScope::Account(login) => Some(login.as_str()),
            AiMeteredScope::Organization => None,
        };
        let body = self
            .get(&credits_path(self.organization(), period, user), user)
            .await?;
        Ok(parse_credits(&body, self.organization(), scope, period)?)
    }

    async fn org_usage(
        &self,
        period: AiMeteredPeriod,
    ) -> Result<Vec<AiMeteredOrgCharge>, MeteredUsageSourceError> {
        let body = self
            .get(&usage_path(self.organization(), period), None)
            .await?;
        Ok(parse_usage(&body, self.organization(), period)?)
    }

    async fn summary(
        &self,
        month: AiBillingMonth,
    ) -> Result<Vec<AiMeteredSummaryLine>, MeteredUsageSourceError> {
        let body = self
            .get(&summary_path(self.organization(), month), None)
            .await?;
        Ok(parse_summary(&body, self.organization(), month)?)
    }
}

#[cfg(test)]
mod tests;
