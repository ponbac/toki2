//! Exchange rates from the Riksbank's SWEA API (`https://api.riksbank.se/swea/v1`).
//!
//! - Each currency's rate against SEK is the series `SEK{CODE}PMI`, such as
//!   `SEKUSDPMI`: SEK per one unit of the currency. The Riksbank quotes every
//!   currency per one unit since 27 November 2023, and the API serves earlier
//!   observations restated per unit too.
//! - A month's average is `ObservationAggregates/{series}/M/{from}/{to}`, which
//!   has an entry per month only once the month is over. A month in progress
//!   is averaged here from its daily `Observations/{series}/{from}/{to}`.
//! - Rates are published on banking days at 12:15 Swedish time. An unknown
//!   series or a range without observations is `204 No Content`, so a `404`
//!   means a wrong base URL or a moved API: an error, not a missing rate.
//! - Without a subscription key the API allows 5 calls a minute and 1,000 a
//!   day per IP address; a key, sent as `Ocp-Apim-Subscription-Key`, raises
//!   that. A `429` is reported with its `Retry-After`, so billing stops asking
//!   for a while. Billing stores every rate it fetches, so it calls rarely.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use time::{Date, Month, OffsetDateTime, Time, UtcOffset, Weekday};

use crate::domain::{
    models::{AiBillingMonth, AiCurrency, AiExchangeRateValue, AiProvidedRate},
    ports::outbound::ExchangeRateProvider,
    ExchangeRateError,
};

pub const DEFAULT_BASE_URL: &str = "https://api.riksbank.se/swea/v1";
const SUBSCRIPTION_KEY_HEADER: &str = "Ocp-Apim-Subscription-Key";
const TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest `Retry-After` honoured.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

/// The Riksbank as an `ExchangeRateProvider`.
pub struct RiksbankExchangeRateAdapter {
    http: Client,
    base_url: String,
    subscription_key: Option<String>,
}

impl RiksbankExchangeRateAdapter {
    /// `base_url` is the SWEA API root, such as `DEFAULT_BASE_URL`.
    pub fn new(
        base_url: &str,
        subscription_key: Option<String>,
    ) -> Result<Self, ExchangeRateError> {
        let http = Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|error| ExchangeRateError::Unavailable(error.to_string()))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            subscription_key: subscription_key.filter(|key| !key.trim().is_empty()),
        })
    }

    /// The response body, or `None` for no content.
    async fn get(&self, path: &str) -> Result<Option<String>, ExchangeRateError> {
        let url = format!("{}/{path}", self.base_url);
        let mut request = self
            .http
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(key) = &self.subscription_key {
            request = request.header(SUBSCRIPTION_KEY_HEADER, key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ExchangeRateError::Unavailable(error.to_string()))?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(None),
            status if status.is_success() => response
                .text()
                .await
                .map(Some)
                .map_err(|error| ExchangeRateError::Unavailable(error.to_string())),
            StatusCode::TOO_MANY_REQUESTS => Err(ExchangeRateError::RateLimited {
                retry_after: response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(parse_retry_after),
            }),
            StatusCode::NOT_FOUND => Err(ExchangeRateError::Unavailable(format!(
                "{} answered 404 Not Found; SWEA answers unknown series with 204, so check \
                 ai_usage.exchange_rates.base_url",
                self.base_url
            ))),
            StatusCode::BAD_REQUEST => Err(ExchangeRateError::Response(format!(
                "400 Bad Request for {path}"
            ))),
            status => Err(ExchangeRateError::Unavailable(format!(
                "{} answered {status}",
                self.base_url
            ))),
        }
    }
}

/// A `Retry-After` in seconds, at most `MAX_RETRY_AFTER`. The HTTP-date form
/// is not used by SWEA and is ignored.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

/// The SWEA series of a currency's rate in SEK per unit.
fn series_id(currency: &AiCurrency) -> String {
    format!("SEK{}PMI", currency.as_str())
}

/// The last Sunday of `month` in `year`.
fn last_sunday(year: i32, month: Month) -> Option<Date> {
    let last = Date::from_calendar_date(year, month, month.length(year)).ok()?;
    let back = last.weekday().number_days_from_sunday();
    last.checked_sub(time::Duration::days(i64::from(back)))
}

/// Stockholm's UTC offset at `instant`: CEST from 01:00 UTC on the last Sunday
/// of March until 01:00 UTC on the last Sunday of October, otherwise CET.
fn stockholm_offset(instant: OffsetDateTime) -> UtcOffset {
    let instant = instant.to_offset(UtcOffset::UTC);
    let year = instant.year();
    let one_am = Time::from_hms(1, 0, 0).unwrap_or(Time::MIDNIGHT);
    let summer = match (
        last_sunday(year, Month::March),
        last_sunday(year, Month::October),
    ) {
        (Some(start), Some(end)) => {
            start.with_time(one_am).assume_utc() <= instant
                && instant < end.with_time(one_am).assume_utc()
        }
        _ => false,
    };
    UtcOffset::from_hms(if summer { 2 } else { 1 }, 0, 0).unwrap_or(UtcOffset::UTC)
}

/// When the Riksbank publishes the rate of `day`: 12:15 in Stockholm.
fn publication(day: Date) -> OffsetDateTime {
    let noon_utc = day
        .with_time(Time::from_hms(11, 0, 0).unwrap_or(Time::MIDNIGHT))
        .assume_utc();
    day.with_time(Time::from_hms(12, 15, 0).unwrap_or(Time::MIDNIGHT))
        .assume_offset(stockholm_offset(noon_utc))
}

/// Whether the rate of a weekday after `day` is due by `now`. Swedish bank
/// holidays are not known here: on one, a refresh finds nothing new.
pub(crate) fn published_after(day: Date, now: OffsetDateTime) -> bool {
    let today = now.to_offset(stockholm_offset(now)).date();
    let mut candidate = day.next_day();
    while let Some(next) = candidate.filter(|next| *next <= today) {
        let weekend = matches!(next.weekday(), Weekday::Saturday | Weekday::Sunday);
        if !weekend && publication(next) <= now {
            return true;
        }
        candidate = next.next_day();
    }
    false
}

#[async_trait]
impl ExchangeRateProvider for RiksbankExchangeRateAdapter {
    fn name(&self) -> &'static str {
        "riksbank"
    }

    fn may_have_published_after(&self, day: Date, now: OffsetDateTime) -> bool {
        published_after(day, now)
    }

    async fn monthly_average(
        &self,
        currency: &AiCurrency,
        month: AiBillingMonth,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
        let path = format!(
            "ObservationAggregates/{}/M/{}/{}",
            series_id(currency),
            month.first_day(),
            month.last_day()
        );
        match self.get(&path).await? {
            Some(body) => parse_monthly_average(&body, month),
            None => Ok(None),
        }
    }

    async fn daily_average(
        &self,
        currency: &AiCurrency,
        from: Date,
        through: Date,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
        let path = format!("Observations/{}/{from}/{through}", series_id(currency));
        match self.get(&path).await? {
            Some(body) => parse_daily_average(&body, from, through),
            None => Ok(None),
        }
    }
}

fn parse_day(raw: &str) -> Result<Date, ExchangeRateError> {
    Date::parse(
        raw,
        time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ExchangeRateError::Response(format!("invalid date {raw:?}")))
}

/// One month of `ObservationAggregates/{series}/M/…`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonthAggregate {
    year: i32,
    /// The month's number, 1 to 12.
    seq_nr: u8,
    /// The last day with an observation, `YYYY-MM-DD`.
    to: String,
    average: serde_json::Number,
}

/// One banking day of `Observations/{series}/…`.
#[derive(Debug, Deserialize)]
struct Observation {
    /// `YYYY-MM-DD`.
    date: String,
    value: serde_json::Number,
}

/// A JSON number as an exact rate: its shortest decimal form, which is how the
/// Riksbank publishes it (at most six decimals).
fn rate(number: &serde_json::Number) -> Result<AiExchangeRateValue, ExchangeRateError> {
    AiExchangeRateValue::parse(&number.to_string())
        .ok_or_else(|| ExchangeRateError::Response(format!("invalid rate {number}")))
}

/// The average of `month` in an aggregates response, if it has one.
pub(crate) fn parse_monthly_average(
    body: &str,
    month: AiBillingMonth,
) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    let aggregates: Vec<MonthAggregate> = serde_json::from_str(body)
        .map_err(|error| ExchangeRateError::Response(error.to_string()))?;
    let first_day = month.first_day();

    aggregates
        .iter()
        .find(|aggregate| {
            aggregate.year == first_day.year() && aggregate.seq_nr == u8::from(first_day.month())
        })
        .map(|aggregate| {
            Ok(AiProvidedRate {
                rate: rate(&aggregate.average)?,
                observed_through: parse_day(&aggregate.to)?,
            })
        })
        .transpose()
}

/// The mean of the observations from `from` through `through` in an
/// observations response, rounded to a millionth, and the latest day it
/// includes, if there are any.
pub(crate) fn parse_daily_average(
    body: &str,
    from: Date,
    through: Date,
) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    let observations: Vec<Observation> = serde_json::from_str(body)
        .map_err(|error| ExchangeRateError::Response(error.to_string()))?;
    let mut values = Vec::with_capacity(observations.len());
    let mut latest = None;
    for observation in &observations {
        let day = parse_day(&observation.date)?;
        if from <= day && day <= through {
            values.push(rate(&observation.value)?);
            latest = latest.max(Some(day));
        }
    }

    Ok(AiExchangeRateValue::mean(&values)
        .zip(latest)
        .map(|(rate, observed_through)| AiProvidedRate {
            rate,
            observed_through,
        }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        extract::{Path, State},
        http::{HeaderMap, StatusCode as HttpStatus},
        response::{IntoResponse, Response},
        routing::get,
        Router,
    };
    use time::macros::{date, datetime};

    use super::*;

    const USD_AUGUST: &str = include_str!("fixtures/monthly_aggregate_usd.json");
    const EUR_JULY_AND_AUGUST: &str = include_str!("fixtures/monthly_aggregates_two_months.json");
    const JPY_AUGUST: &str = include_str!("fixtures/monthly_aggregate_jpy.json");
    const USD_SEPTEMBER_SO_FAR: &str = include_str!("fixtures/observations_usd_month_to_date.json");

    fn month(raw: &str) -> AiBillingMonth {
        AiBillingMonth::parse(raw).unwrap()
    }

    fn decimal(rate: Option<AiProvidedRate>) -> Option<(String, Date)> {
        rate.map(|rate| (rate.rate.to_decimal(), rate.observed_through))
    }

    fn some(rate: &str, through: Date) -> Option<(String, Date)> {
        Some((rate.to_string(), through))
    }

    #[test]
    fn monthly_averages_are_read_exactly_for_their_month() {
        let august = month("2026-08");
        let last_banking_day = date!(2026 - 08 - 31);
        assert_eq!(
            decimal(parse_monthly_average(USD_AUGUST, august).unwrap()),
            some("9.61234", last_banking_day)
        );
        assert_eq!(
            decimal(parse_monthly_average(EUR_JULY_AND_AUGUST, august).unwrap()),
            some("11.05432", last_banking_day)
        );
        assert_eq!(
            decimal(parse_monthly_average(EUR_JULY_AND_AUGUST, month("2026-07")).unwrap()),
            some("11.12345", date!(2026 - 07 - 31))
        );
        // Quoted per one yen, to six decimals.
        assert_eq!(
            decimal(parse_monthly_average(JPY_AUGUST, august).unwrap()),
            some("0.064321", last_banking_day)
        );
        // A response without the month has no average for it.
        assert_eq!(
            parse_monthly_average(USD_AUGUST, month("2026-09")).unwrap(),
            None
        );
        assert_eq!(parse_monthly_average("", august).unwrap(), None);
        assert_eq!(parse_monthly_average("[]", august).unwrap(), None);
    }

    #[test]
    fn a_month_to_date_average_is_the_mean_of_its_days() {
        // (9.6 + 9.65 + 9.70001) / 3 = 9.6500033…, through the 3rd.
        assert_eq!(
            decimal(
                parse_daily_average(
                    USD_SEPTEMBER_SO_FAR,
                    date!(2026 - 09 - 01),
                    date!(2026 - 09 - 30)
                )
                .unwrap()
            ),
            some("9.650003", date!(2026 - 09 - 03))
        );
        // Only the days asked for.
        assert_eq!(
            decimal(
                parse_daily_average(
                    USD_SEPTEMBER_SO_FAR,
                    date!(2026 - 09 - 01),
                    date!(2026 - 09 - 02)
                )
                .unwrap()
            ),
            some("9.625", date!(2026 - 09 - 02))
        );
        assert_eq!(
            parse_daily_average("[]", date!(2026 - 09 - 01), date!(2026 - 09 - 30)).unwrap(),
            None
        );
    }

    #[test]
    fn malformed_responses_are_errors_not_rates() {
        let august = month("2026-08");
        for body in [
            "{}",
            "not json",
            r#"[{"year":2026,"seqNr":8,"to":"2026-08-31","average":"9.5"}]"#,
            r#"[{"year":2026,"seqNr":8,"to":"2026-08-31","average":-9.5}]"#,
            r#"[{"year":2026,"seqNr":8,"to":"2026-08-31","average":0}]"#,
            r#"[{"year":2026,"seqNr":8,"to":"31/8","average":9.5}]"#,
        ] {
            assert!(parse_monthly_average(body, august).is_err(), "{body}");
        }
        assert!(parse_daily_average(
            r#"[{"date":"2026-09-01","value":null}]"#,
            date!(2026 - 09 - 01),
            date!(2026 - 09 - 30)
        )
        .is_err());
    }

    #[test]
    fn rates_are_published_at_a_quarter_past_noon_in_stockholm_on_weekdays() {
        // Summer time: 12:15 CEST is 10:15 UTC.
        assert_eq!(
            publication(date!(2026 - 09 - 29)),
            datetime!(2026-09-29 10:15 UTC)
        );
        // Winter time: 12:15 CET is 11:15 UTC; also across both changes.
        assert_eq!(
            publication(date!(2026 - 01 - 15)),
            datetime!(2026-01-15 11:15 UTC)
        );
        assert_eq!(
            publication(date!(2026 - 03 - 30)),
            datetime!(2026-03-30 10:15 UTC)
        );
        assert_eq!(
            publication(date!(2026 - 10 - 26)),
            datetime!(2026-10-26 11:15 UTC)
        );

        // Tuesday 29 September: before and after publication.
        let monday = date!(2026 - 09 - 28);
        assert!(!published_after(monday, datetime!(2026-09-29 10:14 UTC)));
        assert!(published_after(monday, datetime!(2026-09-29 10:15 UTC)));
        // Nothing is published on a weekend: Friday's rate is the latest until
        // Monday's publication.
        let friday = date!(2026 - 09 - 25);
        assert!(!published_after(friday, datetime!(2026-09-27 20:00 UTC)));
        assert!(!published_after(friday, datetime!(2026-09-28 10:00 UTC)));
        assert!(published_after(friday, datetime!(2026-09-28 10:30 UTC)));
        // Rates older than yesterday: a newer one is due.
        assert!(published_after(
            date!(2026 - 09 - 01),
            datetime!(2026-09-29 08:00 UTC)
        ));
    }

    #[test]
    fn retry_after_is_read_in_seconds_and_capped() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after(" 7 "), Some(Duration::from_secs(7)));
        assert_eq!(parse_retry_after("86400"), Some(MAX_RETRY_AFTER));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }

    /// What the fake SWEA API received.
    #[derive(Clone, Default)]
    struct Received {
        paths: Arc<Mutex<Vec<String>>>,
        keys: Arc<Mutex<Vec<Option<String>>>>,
    }

    /// A local stand-in for the SWEA API that serves the fixtures. GBP is
    /// rate-limited, CHF fails, and NOK is served from a moved path.
    async fn fake_swea() -> (String, Received) {
        async fn serve(
            State(received): State<Received>,
            Path(path): Path<String>,
            headers: HeaderMap,
        ) -> Response {
            received.paths.lock().unwrap().push(path.clone());
            received.keys.lock().unwrap().push(
                headers
                    .get(SUBSCRIPTION_KEY_HEADER)
                    .map(|key| key.to_str().unwrap().to_string()),
            );
            match path.as_str() {
                "ObservationAggregates/SEKUSDPMI/M/2026-08-01/2026-08-31" => {
                    USD_AUGUST.into_response()
                }
                "Observations/SEKUSDPMI/2026-09-01/2026-09-30" => {
                    USD_SEPTEMBER_SO_FAR.into_response()
                }
                path if path.contains("SEKGBPPMI") => {
                    (HttpStatus::TOO_MANY_REQUESTS, [("retry-after", "30")]).into_response()
                }
                path if path.contains("SEKCHFPMI") => {
                    HttpStatus::SERVICE_UNAVAILABLE.into_response()
                }
                path if path.contains("SEKNOKPMI") => HttpStatus::NOT_FOUND.into_response(),
                _ => HttpStatus::NO_CONTENT.into_response(),
            }
        }

        let received = Received::default();
        let app = Router::new()
            .route("/swea/v1/{*path}", get(serve))
            .with_state(received.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        (format!("http://{address}/swea/v1/"), received)
    }

    fn currency(code: &str) -> AiCurrency {
        AiCurrency::parse(code).unwrap()
    }

    #[tokio::test]
    async fn the_adapter_calls_the_swea_endpoints_with_its_key() {
        let (base_url, received) = fake_swea().await;
        let adapter =
            RiksbankExchangeRateAdapter::new(&base_url, Some("synthetic-key".to_string())).unwrap();
        let usd = AiCurrency::usd();

        assert_eq!(
            decimal(
                adapter
                    .monthly_average(&usd, month("2026-08"))
                    .await
                    .unwrap()
            ),
            some("9.61234", date!(2026 - 08 - 31))
        );
        // A month in progress has no aggregate yet: 204.
        assert_eq!(
            adapter
                .monthly_average(&usd, month("2026-09"))
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            decimal(
                adapter
                    .daily_average(&usd, date!(2026 - 09 - 01), date!(2026 - 09 - 30))
                    .await
                    .unwrap()
            ),
            some("9.650003", date!(2026 - 09 - 03))
        );
        // An unknown series is 204 too.
        assert_eq!(
            adapter
                .monthly_average(&currency("XTS"), month("2026-08"))
                .await
                .unwrap(),
            None
        );

        assert_eq!(
            received.paths.lock().unwrap().as_slice(),
            [
                "ObservationAggregates/SEKUSDPMI/M/2026-08-01/2026-08-31",
                "ObservationAggregates/SEKUSDPMI/M/2026-09-01/2026-09-30",
                "Observations/SEKUSDPMI/2026-09-01/2026-09-30",
                "ObservationAggregates/SEKXTSPMI/M/2026-08-01/2026-08-31",
            ]
        );
        assert!(received
            .keys
            .lock()
            .unwrap()
            .iter()
            .all(|key| key.as_deref() == Some("synthetic-key")));
    }

    #[tokio::test]
    async fn provider_failures_are_errors_never_missing_rates() {
        let (base_url, _) = fake_swea().await;
        let adapter = RiksbankExchangeRateAdapter::new(&base_url, None).unwrap();
        let august = month("2026-08");

        // Rate-limited, with how long to wait.
        let limited = adapter
            .monthly_average(&currency("GBP"), august)
            .await
            .unwrap_err();
        assert!(matches!(
            limited,
            ExchangeRateError::RateLimited {
                retry_after: Some(wait)
            } if wait == Duration::from_secs(30)
        ));
        assert!(limited.is_provider_wide());
        // A server error, and a 404 from a moved API, concern the provider.
        for code in ["CHF", "NOK"] {
            let error = adapter
                .monthly_average(&currency(code), august)
                .await
                .unwrap_err();
            assert!(
                matches!(error, ExchangeRateError::Unavailable(_)) && error.is_provider_wide(),
                "{code}: {error}"
            );
        }
        let not_found = adapter
            .daily_average(
                &currency("NOK"),
                date!(2026 - 08 - 01),
                date!(2026 - 08 - 31),
            )
            .await
            .unwrap_err();
        assert!(not_found.to_string().contains("404"), "{not_found}");
        assert!(not_found.to_string().contains("base_url"), "{not_found}");

        // Nothing listening: unavailable.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed = format!("http://{}/swea/v1", listener.local_addr().unwrap());
        drop(listener);
        let unreachable = RiksbankExchangeRateAdapter::new(&closed, None).unwrap();
        assert!(matches!(
            unreachable
                .monthly_average(&AiCurrency::usd(), august)
                .await,
            Err(ExchangeRateError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn without_a_key_no_key_header_is_sent() {
        let (base_url, received) = fake_swea().await;
        let adapter = RiksbankExchangeRateAdapter::new(&base_url, Some(" ".to_string())).unwrap();
        adapter
            .monthly_average(&AiCurrency::usd(), month("2026-08"))
            .await
            .unwrap();
        assert_eq!(received.keys.lock().unwrap().as_slice(), [None]);
    }
}
