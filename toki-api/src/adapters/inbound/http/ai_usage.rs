//! Uploads from `token-ledger sync`: hourly, per-session AI token usage per machine.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, FromRef, Path, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::put,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    adapters::inbound::http::ErrorResponse,
    auth::AuthUser,
    domain::{
        models::{
            AiCoverageStatus, AiMachine, AiMachineId, AiPricing, AiPricingStatus, AiProvider,
            AiProviderCoverage, AiProviderHint, AiTokenCounts, AiUsageBucket, AiUsageUpload,
            AiUsageWindow,
        },
        ports::inbound::AiUsageService,
    },
    routes::ApiError,
};

/// A two-week upload is typically a few hundred kilobytes, but a long window can
/// reach several megabytes, beyond Axum's 2 MB default.
const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const SUPPORTED_VERSIONS: [u64; 1] = [1];

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiUsageService>: FromRef<S>,
{
    Router::new().route(
        "/machines/{machine_id}/usage",
        put(upload_usage).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
    )
}

/// Version 1 upload from `token-ledger sync`. All instants are RFC 3339 UTC.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageUploadRequest {
    /// Payload format version. Only `1` is supported.
    #[schema(value_type = u64, minimum = 1, maximum = 1)]
    version: Value,
    machine: AiUsageMachinePayload,
    /// token-ledger version, for diagnostics.
    #[schema(min_length = 1, max_length = 512)]
    client_version: String,
    /// The client's IANA time zone, for display only.
    #[schema(min_length = 1, max_length = 512)]
    time_zone: String,
    window: AiUsageWindowPayload,
    #[allow(dead_code, reason = "validated by deserialization")]
    currency: AiUsageCurrency,
    #[allow(dead_code, reason = "validated by deserialization")]
    cost_basis: AiUsageCostBasis,
    pricing: AiUsagePricingPayload,
    buckets: Vec<AiUsageBucketPayload>,
    /// One entry per scanned provider. Only providers reported as `ok` or
    /// `partial` are replaced, and every bucket and hint must belong to one.
    coverage: Vec<AiUsageCoveragePayload>,
    #[serde(default)]
    provider_hints: Vec<AiUsageProviderHintPayload>,
}

/// The uploading installation.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageMachinePayload {
    /// Random UUID created once per installation; equals the path `machine_id`.
    #[schema(format = "uuid")]
    id: String,
    /// Editable display name, usually the host name.
    #[schema(min_length = 1, max_length = 512)]
    label: String,
}

/// The half-open `[start, end)` range of whole UTC hours that this upload replaces
/// for its covered providers.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageWindowPayload {
    #[serde(deserialize_with = "utc_instant::required")]
    #[schema(value_type = String, format = DateTime)]
    start: OffsetDateTime,
    #[serde(deserialize_with = "utc_instant::required")]
    #[schema(value_type = String, format = DateTime)]
    end: OffsetDateTime,
}

#[derive(Debug, Deserialize, ToSchema)]
pub enum AiUsageCurrency {
    #[serde(rename = "USD")]
    Usd,
}

/// Costs are API-equivalent estimates, not subscription bills.
#[derive(Debug, Deserialize, ToSchema)]
pub enum AiUsageCostBasis {
    #[serde(rename = "api-equivalent")]
    ApiEquivalent,
}

/// Provenance of the rates behind every cost estimate.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsagePricingPayload {
    status: AiUsagePricingStatus,
    /// When the rates were fetched, or null.
    #[serde(deserialize_with = "utc_instant::nullable")]
    #[schema(value_type = Option<String>, format = DateTime, required = true)]
    fetched_at: Option<OffsetDateTime>,
    #[schema(max_length = 512)]
    source: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsagePricingStatus {
    Fresh,
    Cached,
    Unavailable,
    Custom,
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsageProvider {
    Codex,
    Claude,
    Grok,
    Copilot,
}

/// One session's usage of one model during one UTC hour. The key
/// `(hourStart, sessionKey, project, provider, model)` is unique per upload.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageBucketPayload {
    /// A whole UTC hour inside the window.
    #[serde(deserialize_with = "utc_instant::required")]
    #[schema(value_type = String, format = DateTime)]
    hour_start: OffsetDateTime,
    /// 32 lowercase hex characters; opaque.
    #[schema(pattern = "^[0-9a-f]{32}$")]
    session_key: String,
    /// A configured project name, `host/owner/repo`, or `unattributed`.
    #[schema(min_length = 1, max_length = 512)]
    project: String,
    provider: AiUsageProvider,
    #[schema(max_length = 512)]
    model: String,
    tokens: AiUsageTokensPayload,
    #[schema(minimum = 1)]
    records: i64,
    /// API-equivalent USD, or null exactly when `unpricedRecords` is positive.
    /// Null is unknown, never zero.
    #[serde(deserialize_with = "Option::deserialize")]
    #[schema(required = true, minimum = 0)]
    estimated_cost_usd: Option<f64>,
    /// Records without a known price; at most `records`.
    #[schema(minimum = 0)]
    unpriced_records: i64,
}

/// Disjoint counts: `input` excludes cache reads and writes; reasoning is inside `output`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageTokensPayload {
    #[schema(minimum = 0)]
    input: i64,
    #[schema(minimum = 0)]
    cache_read: i64,
    #[schema(minimum = 0)]
    cache_write: i64,
    #[schema(minimum = 0)]
    output: i64,
}

/// One provider's history coverage on the uploading machine.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageCoveragePayload {
    provider: AiUsageProvider,
    status: AiUsageCoverageStatus,
    #[schema(minimum = 0)]
    files: i64,
    /// Existing history paths that could not be read.
    #[schema(minimum = 0)]
    unreadable: i64,
    #[schema(minimum = 0)]
    malformed_lines: i64,
    #[schema(minimum = 0)]
    skipped_records: i64,
    #[schema(minimum = 0)]
    duplicates: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsageCoverageStatus {
    Ok,
    Partial,
    Failed,
    Missing,
}

/// A billing plan a provider reported during an hour of the window.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProviderHintPayload {
    provider: AiUsageProvider,
    #[serde(deserialize_with = "utc_instant::required")]
    #[schema(value_type = String, format = DateTime)]
    hour_start: OffsetDateTime,
    #[schema(min_length = 1, max_length = 512)]
    plan: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageUploadResponse {
    /// Buckets now stored for the window.
    stored_buckets: u64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedAiUsageVersionResponse {
    error: String,
    supported_versions: Vec<u64>,
}

/// Upload AI usage for a machine
///
/// Stores hourly, per-session AI token usage from `token-ledger sync`. The first
/// upload registers the machine to the caller. For each provider that `coverage`
/// reports as `ok` or `partial`, the upload atomically replaces the machine's
/// usage of that provider inside `window`; other providers keep their stored
/// usage. Repeating an upload is harmless. Costs are API-equivalent USD
/// estimates; null is unknown, never zero.
#[utoipa::path(
    put,
    path = "/ai-usage/machines/{machine_id}/usage",
    operation_id = "uploadAiUsage",
    tag = "AI usage",
    params(
        ("machine_id" = String, Path, format = "uuid", description = "The machine ID; equals `machine.id` in the body")
    ),
    request_body = AiUsageUploadRequest,
    responses(
        (status = 200, description = "Usage stored", body = AiUsageUploadResponse),
        (status = 400, description = "The body is not a valid payload or names another machine", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 403, description = "The machine is registered to another user", body = ErrorResponse),
        (status = 413, description = "The body exceeds 16 MiB"),
        (status = 415, description = "The body is not JSON"),
        (status = 422, description = "Unsupported payload version", body = UnsupportedAiUsageVersionResponse),
        (status = 500, description = "Usage could not be stored", body = ErrorResponse)
    )
)]
pub async fn upload_usage(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageService>>,
    Path(machine_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AiUsageUploadResponse>, UploadRejection> {
    if !is_json(&headers) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected an application/json body",
        )
        .into());
    }

    let request = match serde_json::from_slice::<AiUsageUploadRequest>(&body) {
        Ok(request) => {
            check_version(Some(&request.version))?;
            request
        }
        Err(error) => {
            // Only a rejected body is read a second time: a newer client should
            // learn which versions are supported rather than read validation
            // errors for a format it did not send.
            if let Ok(probe) = serde_json::from_slice::<VersionProbe>(&body) {
                check_version(probe.version.as_ref())?;
            }
            return Err(ApiError::bad_request(format!("invalid AI usage payload: {error}")).into());
        }
    };

    let path_machine_id = AiMachineId::parse(&machine_id)
        .ok_or_else(|| ApiError::bad_request("machine_id must be a UUID"))?;
    let upload = request.into_upload()?;
    if upload.machine().id != path_machine_id {
        return Err(
            ApiError::bad_request("machine.id must equal the machine_id in the path").into(),
        );
    }

    let receipt = service
        .ingest(&user.id, &upload)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AiUsageUploadResponse {
        stored_buckets: receipt.stored_buckets,
    }))
}

/// Why an upload was refused.
pub enum UploadRejection {
    Api(ApiError),
    /// A well-formed version this server does not support.
    UnsupportedVersion(u64),
}

impl From<ApiError> for UploadRejection {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl IntoResponse for UploadRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Api(error) => error.into_response(),
            Self::UnsupportedVersion(version) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(UnsupportedAiUsageVersionResponse {
                    error: format!("unsupported AI usage payload version {version}"),
                    supported_versions: SUPPORTED_VERSIONS.to_vec(),
                }),
            )
                .into_response(),
        }
    }
}

/// A missing or malformed version is a bad request; only a well-formed version
/// that is not supported is `UnsupportedVersion`.
fn check_version(version: Option<&Value>) -> Result<(), UploadRejection> {
    let version = version
        .filter(|version| !version.is_null())
        .ok_or_else(|| ApiError::bad_request("version is required"))?;
    let version = version
        .as_u64()
        .ok_or_else(|| ApiError::bad_request("version must be a non-negative integer"))?;

    if SUPPORTED_VERSIONS.contains(&version) {
        Ok(())
    } else {
        Err(UploadRejection::UnsupportedVersion(version))
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    version: Option<Value>,
}

/// Instants in the payload are RFC 3339 with a UTC offset, as token-ledger sends them.
mod utc_instant {
    use serde::{de::Error, Deserialize, Deserializer};
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    pub fn required<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<OffsetDateTime, D::Error> {
        parse(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }

    /// Requires the key but allows null.
    pub fn nullable<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<OffsetDateTime>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| parse(&text))
            .transpose()
            .map_err(D::Error::custom)
    }

    fn parse(text: &str) -> Result<OffsetDateTime, String> {
        OffsetDateTime::parse(text, &Rfc3339)
            .ok()
            .filter(|instant| instant.offset().is_utc())
            .ok_or_else(|| format!("expected an RFC 3339 UTC instant, got {text:?}"))
    }
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|mime| mime.trim().to_ascii_lowercase())
        .is_some_and(|mime| mime == "application/json" || mime.ends_with("+json"))
}

impl AiUsageUploadRequest {
    fn into_upload(self) -> Result<AiUsageUpload, ApiError> {
        let machine_id = AiMachineId::parse(&self.machine.id)
            .ok_or_else(|| ApiError::bad_request("machine.id must be a UUID"))?;
        let machine = AiMachine {
            id: machine_id,
            label: self.machine.label,
            client_version: self.client_version,
            time_zone: self.time_zone,
        };
        let window = AiUsageWindow::new(self.window.start, self.window.end)?;
        let pricing = AiPricing {
            status: match self.pricing.status {
                AiUsagePricingStatus::Fresh => AiPricingStatus::Fresh,
                AiUsagePricingStatus::Cached => AiPricingStatus::Cached,
                AiUsagePricingStatus::Unavailable => AiPricingStatus::Unavailable,
                AiUsagePricingStatus::Custom => AiPricingStatus::Custom,
            },
            fetched_at: self.pricing.fetched_at,
            source: self.pricing.source,
        };
        let buckets = self
            .buckets
            .into_iter()
            .map(|bucket| AiUsageBucket {
                hour_start: bucket.hour_start,
                session_key: bucket.session_key,
                project_key: bucket.project,
                provider: bucket.provider.into(),
                model: bucket.model,
                tokens: AiTokenCounts {
                    input: bucket.tokens.input,
                    cache_read: bucket.tokens.cache_read,
                    cache_write: bucket.tokens.cache_write,
                    output: bucket.tokens.output,
                },
                records: bucket.records,
                estimated_cost_usd: bucket.estimated_cost_usd,
                unpriced_records: bucket.unpriced_records,
            })
            .collect();
        let coverage = self
            .coverage
            .into_iter()
            .map(|entry| AiProviderCoverage {
                provider: entry.provider.into(),
                status: match entry.status {
                    AiUsageCoverageStatus::Ok => AiCoverageStatus::Ok,
                    AiUsageCoverageStatus::Partial => AiCoverageStatus::Partial,
                    AiUsageCoverageStatus::Failed => AiCoverageStatus::Failed,
                    AiUsageCoverageStatus::Missing => AiCoverageStatus::Missing,
                },
                files: entry.files,
                unreadable: entry.unreadable,
                malformed_lines: entry.malformed_lines,
                skipped_records: entry.skipped_records,
                duplicates: entry.duplicates,
            })
            .collect();
        let provider_hints = self
            .provider_hints
            .into_iter()
            .map(|hint| AiProviderHint {
                provider: hint.provider.into(),
                hour_start: hint.hour_start,
                plan: hint.plan,
            })
            .collect();

        Ok(AiUsageUpload::new(
            machine,
            window,
            pricing,
            buckets,
            coverage,
            provider_hints,
        )?)
    }
}

impl From<AiUsageProvider> for AiProvider {
    fn from(provider: AiUsageProvider) -> Self {
        match provider {
            AiUsageProvider::Codex => Self::Codex,
            AiUsageProvider::Claude => Self::Claude,
            AiUsageProvider::Grok => Self::Grok,
            AiUsageProvider::Copilot => Self::Copilot,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use axum::{
        body::{to_bytes, Body},
        http::{header::AUTHORIZATION, Method, Request},
        middleware,
    };
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;

    use crate::{
        adapters::outbound::postgres::{PostgresAiUsageRepository, PostgresApiTokenRepository},
        auth::{authenticate_bearer, require_authenticated},
        domain::{
            models::{
                AiUsageDateRange, AiUsageIngestReceipt, AiUsagePeriod, AiUsagePeriodTotals,
                AiUsageTimeZone, UserId,
            },
            ports::inbound::{ApiTokenAuthenticator, ApiTokenService},
            services::{AiUsageServiceImpl, ApiTokenServiceImpl},
            AiUsageError, ApiTokenError, Role, UserPrincipal,
        },
    };

    use super::*;

    const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";

    #[derive(Default)]
    struct RecordingService {
        uploads: Mutex<Vec<AiUsageUpload>>,
    }

    #[async_trait]
    impl AiUsageService for RecordingService {
        async fn ingest(
            &self,
            _user_id: &UserId,
            upload: &AiUsageUpload,
        ) -> Result<AiUsageIngestReceipt, AiUsageError> {
            self.uploads.lock().unwrap().push(upload.clone());
            Ok(AiUsageIngestReceipt {
                stored_buckets: upload.buckets().len() as u64,
            })
        }

        async fn period_totals(
            &self,
            _user_id: &UserId,
            _dates: AiUsageDateRange,
            _period: AiUsagePeriod,
        ) -> Result<Vec<AiUsagePeriodTotals>, AiUsageError> {
            unreachable!("uploads do not read totals")
        }
    }

    struct AnyTokenAuthenticator;

    #[async_trait]
    impl ApiTokenAuthenticator for AnyTokenAuthenticator {
        async fn authenticate(
            &self,
            _presented: &str,
        ) -> Result<Option<UserPrincipal>, ApiTokenError> {
            Ok(Some(UserPrincipal {
                id: UserId::new(7),
                email: "dev@example.com".to_string(),
                roles: vec![Role::User],
            }))
        }
    }

    /// The route behind the same bearer authentication as protected routes.
    fn app(
        service: Arc<dyn AiUsageService>,
        authenticator: Arc<dyn ApiTokenAuthenticator>,
    ) -> Router {
        Router::new()
            .nest("/ai-usage", router())
            .route_layer(middleware::from_fn(require_authenticated))
            .layer(middleware::from_fn_with_state(
                authenticator,
                authenticate_bearer,
            ))
            .with_state(service)
    }

    /// A synthetic payload with `buckets` distinct sessions in one hour.
    fn payload(buckets: usize) -> Value {
        json!({
            "version": 1,
            "machine": { "id": MACHINE, "label": "work-laptop" },
            "clientVersion": "0.2.0",
            "timeZone": "Europe/Stockholm",
            "window": { "start": "2026-09-08T22:00:00.000Z", "end": "2026-09-22T22:00:00.000Z" },
            "currency": "USD",
            "costBasis": "api-equivalent",
            "pricing": {
                "status": "fresh",
                "fetchedAt": "2026-09-22T07:00:00.000Z",
                "source": "https://example.com/prices.json"
            },
            "buckets": (0..buckets).map(|index| json!({
                "hourStart": "2026-09-22T08:00:00.000Z",
                "sessionKey": format!("{index:032x}"),
                "project": "github.com/example/app",
                "provider": "claude",
                "model": "example-model",
                "tokens": { "input": 1200, "cacheRead": 48000, "cacheWrite": 3000, "output": 900 },
                "records": 14,
                "estimatedCostUsd": if index % 2 == 0 { json!(0.0421) } else { Value::Null },
                "unpricedRecords": if index % 2 == 0 { 0 } else { 14 },
            })).collect::<Vec<_>>(),
            "coverage": [
                {
                    "provider": "claude",
                    "status": "ok",
                    "files": 31,
                    "unreadable": 0,
                    "malformedLines": 0,
                    "skippedRecords": 0,
                    "duplicates": 2
                },
                {
                    "provider": "codex",
                    "status": "partial",
                    "files": 4,
                    "unreadable": 0,
                    "malformedLines": 1,
                    "skippedRecords": 0,
                    "duplicates": 0
                }
            ],
            "providerHints": [
                { "provider": "codex", "hourStart": "2026-09-22T08:00:00.000Z", "plan": "pro" }
            ]
        })
    }

    async fn put(
        app: Router,
        machine_id: &str,
        token: Option<&str>,
        body: &Value,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(Method::PUT)
            .uri(format!("/ai-usage/machines/{machine_id}/usage"))
            .header(CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        let response = app
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn an_unsupported_version_is_reported_before_validation() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        let (status, body) = put(
            app,
            MACHINE,
            Some("toki_any"),
            &json!({ "version": 2, "buckets": "a format this server does not know" }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["supportedVersions"], json!([1]));
        assert!(service.uploads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_version_that_is_not_an_integer_is_a_bad_request() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        for version in [json!("1"), json!(1.0), json!(-1)] {
            let mut body = payload(1);
            body["version"] = version.clone();
            let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "version {version}");
            assert_eq!(body["error"], "version must be a non-negative integer");
        }
        assert!(service.uploads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn pricing_fetched_at_is_a_utc_instant_or_null() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        for fetched_at in [json!("yesterday"), json!("2026-09-22T09:00:00.000+02:00")] {
            let mut body = payload(1);
            body["pricing"]["fetchedAt"] = fetched_at.clone();
            let (status, _) = put(app.clone(), MACHINE, Some("toki_any"), &body).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "fetchedAt {fetched_at}");
        }

        let mut unfetched = payload(1);
        unfetched["pricing"]["fetchedAt"] = Value::Null;
        let (status, _) = put(app, MACHINE, Some("toki_any"), &unfetched).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_a_bucket_outside_the_window_or_another_machines_path() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));
        let mut outside = payload(1);
        outside["buckets"][0]["hourStart"] = json!("2026-09-22T22:00:00.000Z");

        let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &outside).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body["error"],
            "buckets[0]: hourStart must be inside the window"
        );

        let (status, body) = put(
            app,
            "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c",
            Some("toki_any"),
            &payload(1),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body["error"],
            "machine.id must equal the machine_id in the path"
        );
        assert!(service.uploads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unreadable_history_cannot_authorize_replacement() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        for coverage_status in ["ok", "partial"] {
            let mut invalid = payload(1);
            invalid["coverage"][0]["status"] = json!(coverage_status);
            invalid["coverage"][0]["unreadable"] = json!(1);
            let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &invalid).await;

            assert_eq!(status, StatusCode::BAD_REQUEST, "{coverage_status}");
            assert_eq!(
                body["error"],
                "coverage[0]: ok or partial coverage must have no unreadable history"
            );
        }
        assert!(service.uploads.lock().unwrap().is_empty());

        let (status, _) = put(app, MACHINE, Some("toki_any"), &payload(1)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(service.uploads.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn accepts_uploads_larger_than_axums_default_body_limit() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));
        let large = payload(12_000);
        assert!(large.to_string().len() > 3 * 1024 * 1024);

        let (status, body) = put(app, MACHINE, Some("toki_any"), &large).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "storedBuckets": 12_000 }));
        let uploads = service.uploads.lock().unwrap();
        let upload = &uploads[0];
        assert_eq!(upload.buckets()[1].estimated_cost_usd, None);
        assert_eq!(upload.provider_hints().len(), 1);
    }

    #[sqlx::test]
    async fn api_tokens_upload_and_another_users_upload_is_forbidden(pool: PgPool) {
        let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
        let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
            PostgresApiTokenRepository::new(db.clone()),
        )));
        let service: Arc<dyn AiUsageService> = Arc::new(AiUsageServiceImpl::new(
            Arc::new(PostgresAiUsageRepository::new(db)),
            AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
            None,
        ));
        let app = app(service, tokens.clone());

        let mut secrets = Vec::new();
        for email in ["owner@example.com", "other@example.com"] {
            let id: i32 = sqlx::query_scalar(
                "INSERT INTO users (email, full_name, picture, access_token)
                 VALUES ($1, 'Test User', '', '')
                 RETURNING id",
            )
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();
            let issued = tokens
                .create(&UserId::new(id), "token-ledger")
                .await
                .unwrap();
            secrets.push(issued.secret.as_str().to_string());
        }

        let (status, body) = put(app.clone(), MACHINE, Some(&secrets[0]), &payload(2)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "storedBuckets": 2 }));

        let (status, _) = put(app.clone(), MACHINE, Some(&secrets[1]), &payload(2)).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = put(app, MACHINE, None, &payload(2)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
