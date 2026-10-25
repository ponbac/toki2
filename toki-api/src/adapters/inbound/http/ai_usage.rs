//! Uploads from `token-ledger sync`: hourly, per-session AI token usage per machine,
//! and reprice-only uploads that give stored usage new costs (`repricing`).

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, FromRef, Path, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
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
            AiCoverageStatus, AiMachine, AiMachineId, AiPartialBucketCost, AiPricing,
            AiPricingBasis, AiPricingStatus, AiProvider, AiProviderCoverage, AiProviderHint,
            AiTokenCounts, AiUsageBucket, AiUsageUpload, AiUsageWindow, MAX_PRICING_BASIS_CHARS,
        },
        ports::inbound::AiUsageService,
    },
    routes::ApiError,
};

pub(super) mod repricing;

/// A two-week upload is typically a few hundred kilobytes, but a long window can
/// reach several megabytes, beyond Axum's 2 MB default.
const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
const SUPPORTED_VERSIONS: [u64; 3] = [1, 2, 3];

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiUsageService>: FromRef<S>,
{
    Router::new()
        .route(
            "/machines/{machine_id}/usage",
            put(upload_usage).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/machines/{machine_id}/repricing",
            get(repricing::get_repricing)
                .post(repricing::reprice_usage)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
}

/// Upload from `token-ledger sync`. All instants are RFC 3339 UTC.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageUploadRequest {
    /// Payload format version. Versions `1`, `2` and `3` are supported. Version
    /// `2` adds `billingProvider`; version `3` adds the priced subtotal of partly
    /// priced buckets in `estimatedCostUsd`.
    #[schema(value_type = u64, minimum = 1, maximum = 3)]
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
    /// When the client started reading the history in this upload, by its own
    /// clock: every record written before it is included. Optional in every
    /// version. When present, an upload whose snapshot is older than the one
    /// already stored for the machine is refused with 409 and changes nothing,
    /// and the upload proves usage complete only until this instant.
    #[serde(default, deserialize_with = "utc_instant::nullable")]
    #[schema(value_type = Option<String>, format = DateTime)]
    snapshot_at: Option<OffsetDateTime>,
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

/// Costs are estimates, not subscription bills. The name predates Copilot
/// pricing: usage Copilot pays for is priced at Copilot's rates, other usage at
/// API prices.
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
    /// Optional in every version: names the pricing method and the rates
    /// behind every cost in the upload, opaque to Toki and compared only for
    /// equality. Stored on each bucket the upload writes and as the machine's
    /// current pricing basis; absent or null stores none on the buckets and
    /// keeps the machine's. Compare it with stored usage through
    /// `getAiUsageRepricing`.
    #[serde(default)]
    #[schema(min_length = 1, max_length = 128)]
    basis: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsagePricingStatus {
    Fresh,
    Cached,
    Unavailable,
    Custom,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsageProvider {
    Codex,
    Claude,
    Grok,
    Copilot,
    Pi,
    OpenCode,
}

/// One session's usage of one model during one UTC hour. The key
/// `(hourStart, sessionKey, project, provider, model, billingProvider)` is unique per upload.
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
    /// Version 2 and later: subscription payer, distinct from the source tool.
    billing_provider: Option<AiUsageProvider>,
    #[schema(max_length = 512)]
    model: String,
    tokens: AiUsageTokensPayload,
    #[schema(minimum = 1)]
    records: i64,
    /// Estimated USD of the priced records (see `costBasis`). In versions `1`
    /// and `2`, null exactly when `unpricedRecords` is positive. In version `3`,
    /// null exactly when every record is unpriced, and otherwise the priced
    /// subtotal, which excludes any unpriced records. Null is unknown, never zero.
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

/// A billing plan a provider reported during an hour of the window. Pi names
/// no plan: its hint is `openai-codex`, the route of its ChatGPT sign-in, for an
/// hour in which usage ran through that sign-in. OpenCode reports no hints: its
/// buckets name the subscription that pays for them in `billingProvider`.
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
    /// Buckets this upload stored. Buckets kept for providers it did not replace
    /// are not counted.
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
/// usage. Repeating an upload is harmless. An upload with a `snapshotAt` older
/// than one already stored for the machine is refused with 409. Costs are USD
/// estimates: at Copilot's rates for usage Copilot pays for, and otherwise at
/// API prices, OpenAI's implicit cache writes included. Null is unknown, never
/// zero. From version 3, a bucket with unpriced records can still carry the
/// cost of its priced records.
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
        (status = 409, description = "A newer snapshot from this machine is already stored; nothing changed", body = ErrorResponse),
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

    let (request, version) = match serde_json::from_slice::<AiUsageUploadRequest>(&body) {
        Ok(request) => {
            let version = check_version(Some(&request.version))?;
            (request, version)
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
    let upload = request.into_upload(version)?;
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

/// Returns the supported version. A missing or malformed version is a bad
/// request; only a well-formed version that is not supported is
/// `UnsupportedVersion`.
fn check_version(version: Option<&Value>) -> Result<u64, UploadRejection> {
    let version = version
        .filter(|version| !version.is_null())
        .ok_or_else(|| ApiError::bad_request("version is required"))?;
    let version = version
        .as_u64()
        .ok_or_else(|| ApiError::bad_request("version must be a non-negative integer"))?;

    if SUPPORTED_VERSIONS.contains(&version) {
        Ok(version)
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
    /// Converts a request whose `version` is supported.
    fn into_upload(self, version: u64) -> Result<AiUsageUpload, ApiError> {
        if version < 2
            && self
                .buckets
                .iter()
                .any(|bucket| bucket.billing_provider.is_some())
        {
            return Err(ApiError::bad_request(
                "billingProvider requires payload version 2 or later",
            ));
        }
        let partial_cost = if version >= 3 {
            AiPartialBucketCost::PricedSubtotal
        } else {
            AiPartialBucketCost::Omitted
        };
        let machine_id = AiMachineId::parse(&self.machine.id)
            .ok_or_else(|| ApiError::bad_request("machine.id must be a UUID"))?;
        let machine = AiMachine {
            id: machine_id,
            label: self.machine.label,
            client_version: self.client_version,
            time_zone: self.time_zone,
        };
        let window = AiUsageWindow::new(self.window.start, self.window.end)?;
        let pricing_basis = self.pricing.basis.map(pricing_basis).transpose()?;
        let pricing = AiPricing {
            status: self.pricing.status.into(),
            fetched_at: self.pricing.fetched_at,
            source: self.pricing.source,
        };
        let buckets = self.buckets.into_iter().map(Into::into).collect();
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
            partial_cost,
        )?
        .with_pricing_basis(pricing_basis)
        .with_snapshot_at(self.snapshot_at))
    }
}

/// A `pricing.basis` from a request body.
fn pricing_basis(raw: String) -> Result<AiPricingBasis, ApiError> {
    AiPricingBasis::parse(raw).ok_or_else(|| {
        ApiError::bad_request(format!(
            "pricing.basis must be 1 to {MAX_PRICING_BASIS_CHARS} characters without NUL"
        ))
    })
}

impl From<AiUsageBucketPayload> for AiUsageBucket {
    fn from(bucket: AiUsageBucketPayload) -> Self {
        Self {
            billing_provider: bucket.billing_provider.map(Into::into),
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
        }
    }
}

impl From<AiUsagePricingStatus> for AiPricingStatus {
    fn from(status: AiUsagePricingStatus) -> Self {
        match status {
            AiUsagePricingStatus::Fresh => Self::Fresh,
            AiUsagePricingStatus::Cached => Self::Cached,
            AiUsagePricingStatus::Unavailable => Self::Unavailable,
            AiUsagePricingStatus::Custom => Self::Custom,
        }
    }
}

impl From<AiUsageProvider> for AiProvider {
    fn from(provider: AiUsageProvider) -> Self {
        match provider {
            AiUsageProvider::Codex => Self::Codex,
            AiUsageProvider::Claude => Self::Claude,
            AiUsageProvider::Grok => Self::Grok,
            AiUsageProvider::Copilot => Self::Copilot,
            AiUsageProvider::Pi => Self::Pi,
            AiUsageProvider::OpenCode => Self::OpenCode,
        }
    }
}

impl From<AiProvider> for AiUsageProvider {
    fn from(provider: AiProvider) -> Self {
        match provider {
            AiProvider::Codex => Self::Codex,
            AiProvider::Claude => Self::Claude,
            AiProvider::Grok => Self::Grok,
            AiProvider::Copilot => Self::Copilot,
            AiProvider::Pi => Self::Pi,
            AiProvider::OpenCode => Self::OpenCode,
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
    use time::macros::datetime;
    use tower::ServiceExt;

    use crate::{
        adapters::outbound::postgres::{PostgresAiUsageRepository, PostgresApiTokenRepository},
        auth::{authenticate_bearer, require_authenticated},
        domain::{
            models::{
                AiMachineId, AiRepricing, AiRepricingReceipt, AiRepricingStatus,
                AiUsageIngestReceipt, UserId,
            },
            ports::inbound::{ApiTokenAuthenticator, ApiTokenService},
            services::{AiUsageServiceImpl, ApiTokenServiceImpl},
            AiUsageError, ApiTokenError, Role, UserPrincipal,
        },
    };

    use super::*;

    pub(super) const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
    /// The reprice floor of `postgres_service`: inside `payload`'s window,
    /// after the hour of its buckets.
    pub(super) const REPRICE_FLOOR: OffsetDateTime = datetime!(2026-09-22 09:00 UTC);

    #[derive(Default)]
    pub(super) struct RecordingService {
        pub(super) uploads: Mutex<Vec<AiUsageUpload>>,
        pub(super) reprices: Mutex<Vec<AiRepricing>>,
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

        async fn repricing_status(
            &self,
            _user_id: &UserId,
            _machine_id: &AiMachineId,
            _basis: &AiPricingBasis,
        ) -> Result<AiRepricingStatus, AiUsageError> {
            Ok(AiRepricingStatus {
                floor: REPRICE_FLOOR,
                providers: Vec::new(),
            })
        }

        async fn reprice(
            &self,
            _user_id: &UserId,
            repricing: &AiRepricing,
        ) -> Result<AiRepricingReceipt, AiUsageError> {
            let (repriced, before_floor) = repricing.split_at_floor(REPRICE_FLOOR);
            let receipt = AiRepricingReceipt {
                updated: repriced.len() as u64,
                unmatched: 0,
                before_floor,
            };
            self.reprices.lock().unwrap().push(repricing.clone());
            Ok(receipt)
        }
    }

    /// The service over Postgres, with `REPRICE_FLOOR`.
    pub(super) fn postgres_service(pool: &PgPool) -> Arc<dyn AiUsageService> {
        Arc::new(AiUsageServiceImpl::new(
            Arc::new(PostgresAiUsageRepository::new(crate::db::DbPool::from(
                pool.clone(),
            ))),
            REPRICE_FLOOR,
        ))
    }

    pub(super) struct AnyTokenAuthenticator;

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
    pub(super) fn app(
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
    pub(super) fn payload(buckets: usize) -> Value {
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

    pub(super) async fn put(
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
    async fn billing_overrides_need_v2_and_keep_mixed_source_buckets_distinct() {
        let service = Arc::new(RecordingService::default());
        let mut body = payload(1);
        body["buckets"][0]["billingProvider"] = json!("codex");
        let (status, _) = put(
            app(service.clone(), Arc::new(AnyTokenAuthenticator)),
            MACHINE,
            Some("toki_any"),
            &body,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        body["version"] = json!(2);
        let mut native = body["buckets"][0].clone();
        native.as_object_mut().unwrap().remove("billingProvider");
        body["buckets"].as_array_mut().unwrap().push(native);
        let (status, _) = put(
            app(service.clone(), Arc::new(AnyTokenAuthenticator)),
            MACHINE,
            Some("toki_any"),
            &body,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        {
            let uploads = service.uploads.lock().unwrap();
            let buckets = uploads[0].buckets();
            assert_eq!(buckets.len(), 2);
            assert_eq!(buckets[0].provider, AiProvider::Claude);
            assert_eq!(buckets[0].billing_provider, Some(AiProvider::Codex));
            assert_eq!(buckets[1].billing_provider, None);
        }
        for invalid in ["claude", "pi", "opencode", "unknown"] {
            body["buckets"][0]["billingProvider"] = json!(invalid);
            let (status, _) = put(
                app(service.clone(), Arc::new(AnyTokenAuthenticator)),
                MACHINE,
                Some("toki_any"),
                &body,
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
        }
        assert_eq!(service.uploads.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn only_version_3_carries_the_priced_subtotal_of_a_partly_priced_bucket() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        let mut partly = payload(1);
        partly["buckets"][0]["unpricedRecords"] = json!(4);
        for version in [1, 2] {
            partly["version"] = json!(version);
            let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &partly).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "version {version}");
            assert_eq!(
                body["error"],
                "buckets[0]: estimatedCostUsd must be null exactly when unpricedRecords is positive"
            );
        }
        partly["version"] = json!(3);
        let (status, _) = put(app.clone(), MACHINE, Some("toki_any"), &partly).await;
        assert_eq!(status, StatusCode::OK);

        // An all-unpriced bucket still has no cost, not a zero one.
        let mut unpriced = payload(2);
        unpriced["version"] = json!(3);
        unpriced["buckets"][1]["estimatedCostUsd"] = json!(0.0);
        let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &unpriced).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body["error"],
            "buckets[1]: estimatedCostUsd must be null exactly when every record is unpriced"
        );
        unpriced["buckets"][1]["estimatedCostUsd"] = Value::Null;
        let (status, _) = put(app, MACHINE, Some("toki_any"), &unpriced).await;
        assert_eq!(status, StatusCode::OK);

        let uploads = service.uploads.lock().unwrap();
        let costs: Vec<_> = uploads
            .iter()
            .flat_map(AiUsageUpload::buckets)
            .map(|bucket| (bucket.estimated_cost_usd, bucket.unpriced_records))
            .collect();
        assert_eq!(costs, [(Some(0.0421), 4), (Some(0.0421), 0), (None, 14)]);
    }

    #[tokio::test]
    async fn an_unsupported_version_is_reported_before_validation() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        let (status, body) = put(
            app,
            MACHINE,
            Some("toki_any"),
            &json!({ "version": 4, "buckets": "a format this server does not know" }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["supportedVersions"], json!([1, 2, 3]));
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
    async fn pricing_basis_is_optional_and_1_to_128_characters() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        for basis in [json!(""), json!("a".repeat(129))] {
            let mut body = payload(1);
            body["pricing"]["basis"] = basis;
            let (status, body) = put(app.clone(), MACHINE, Some("toki_any"), &body).await;
            assert_eq!(
                (status, &body["error"]),
                (
                    StatusCode::BAD_REQUEST,
                    &json!("pricing.basis must be 1 to 128 characters without NUL")
                )
            );
        }
        assert!(service.uploads.lock().unwrap().is_empty());

        let mut null = payload(1);
        null["pricing"]["basis"] = Value::Null;
        let mut longest = payload(1);
        longest["pricing"]["basis"] = json!("a".repeat(128));
        for body in [payload(1), null, longest] {
            let (status, _) = put(app.clone(), MACHINE, Some("toki_any"), &body).await;
            assert_eq!(status, StatusCode::OK);
        }

        let bases: Vec<_> = service
            .uploads
            .lock()
            .unwrap()
            .iter()
            .map(|upload| upload.pricing_basis().map(|basis| basis.as_str().len()))
            .collect();
        assert_eq!(bases, [None, None, Some(128)]);
    }

    #[tokio::test]
    async fn snapshot_at_is_an_optional_utc_instant() {
        let service = Arc::new(RecordingService::default());
        let app = app(service.clone(), Arc::new(AnyTokenAuthenticator));

        let mut offset = payload(1);
        offset["snapshotAt"] = json!("2026-09-22T11:00:00.000+02:00");
        let (status, _) = put(app.clone(), MACHINE, Some("toki_any"), &offset).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let mut read = payload(1);
        read["snapshotAt"] = json!("2026-09-22T09:00:00.000Z");
        for body in [payload(1), read] {
            let (status, _) = put(app.clone(), MACHINE, Some("toki_any"), &body).await;
            assert_eq!(status, StatusCode::OK);
        }

        let snapshots: Vec<_> = service
            .uploads
            .lock()
            .unwrap()
            .iter()
            .map(AiUsageUpload::snapshot_at)
            .collect();
        assert_eq!(
            snapshots,
            [None, Some(time::macros::datetime!(2026-09-22 09:00 UTC))]
        );
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
        let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
            PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
        )));
        let service = postgres_service(&pool);
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

    #[sqlx::test]
    async fn an_upload_older_than_the_stored_snapshot_is_a_conflict(pool: PgPool) {
        let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
            PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
        )));
        let service = postgres_service(&pool);
        let app = app(service, tokens.clone());
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ('dev@example.com', 'Test User', '', '')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let issued = tokens
            .create(&UserId::new(id), "token-ledger")
            .await
            .unwrap();
        let secret = issued.secret.as_str();
        let read_at = |snapshot_at: &str| {
            let mut body = payload(1);
            body["snapshotAt"] = json!(snapshot_at);
            body
        };

        let (status, _) = put(
            app.clone(),
            MACHINE,
            Some(secret),
            &read_at("2026-09-22T10:30:00.000Z"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = put(
            app,
            MACHINE,
            Some(secret),
            &read_at("2026-09-22T10:00:00.000Z"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body["error"],
            "a newer upload from this machine is already stored"
        );
    }

    #[sqlx::test]
    async fn pi_usage_is_stored_with_its_coverage_and_sign_in_hint(pool: PgPool) {
        let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
            PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
        )));
        let service = postgres_service(&pool);
        let app = app(service, tokens.clone());
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ('dev@example.com', 'Test User', '', '')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let issued = tokens
            .create(&UserId::new(id), "token-ledger")
            .await
            .unwrap();
        let secret = issued.secret.as_str();
        let coverage = |status: &str| {
            json!([{
                "provider": "pi",
                "status": status,
                "files": 0,
                "unreadable": 0,
                "malformedLines": 0,
                "skippedRecords": 0,
                "duplicates": 0
            }])
        };

        let mut used = payload(1);
        used["buckets"][0]["provider"] = json!("pi");
        used["coverage"] = coverage("ok");
        used["providerHints"] = json!([
            { "provider": "pi", "hourStart": "2026-09-22T08:00:00.000Z", "plan": "openai-codex" }
        ]);
        let (status, body) = put(app.clone(), MACHINE, Some(secret), &used).await;
        assert_eq!(
            (status, body),
            (StatusCode::OK, json!({ "storedBuckets": 1 }))
        );

        // A machine that never ran Pi reports it as missing in every sync.
        let mut unused = payload(0);
        unused["coverage"] = coverage("missing");
        unused["providerHints"] = json!([]);
        let other_machine = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
        unused["machine"]["id"] = json!(other_machine);
        let (status, _) = put(app, other_machine, Some(secret), &unused).await;
        assert_eq!(status, StatusCode::OK);

        let stored: Vec<(String, String)> = sqlx::query_as(
            "SELECT 'bucket', provider FROM ai_usage_buckets
             UNION ALL SELECT 'hint ' || plan, provider FROM ai_usage_provider_hints
             UNION ALL SELECT 'coverage ' || status, provider FROM ai_usage_machine_coverage
             UNION ALL SELECT 'log', provider FROM ai_usage_coverage_log
             ORDER BY 1",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            [
                "bucket",
                "coverage missing",
                "coverage ok",
                "hint openai-codex",
                "log"
            ]
            .map(|row| (row.to_string(), "pi".to_string()))
        );
    }

    #[sqlx::test]
    async fn opencode_usage_is_stored_with_its_payer_and_without_hints(pool: PgPool) {
        let tokens = Arc::new(ApiTokenServiceImpl::new(Arc::new(
            PostgresApiTokenRepository::new(crate::db::DbPool::from(pool.clone())),
        )));
        let service = postgres_service(&pool);
        let app = app(service, tokens.clone());
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token)
             VALUES ('dev@example.com', 'Test User', '', '')
             RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let issued = tokens
            .create(&UserId::new(id), "token-ledger")
            .await
            .unwrap();
        let secret = issued.secret.as_str();
        let coverage = |status: &str| {
            json!([{
                "provider": "opencode",
                "status": status,
                "files": 0,
                "unreadable": 0,
                "malformedLines": 0,
                "skippedRecords": 0,
                "duplicates": 0
            }])
        };

        let mut used = payload(1);
        used["version"] = json!(2);
        used["buckets"][0]["provider"] = json!("opencode");
        used["buckets"][0]["billingProvider"] = json!("copilot");
        used["coverage"] = coverage("ok");
        used["providerHints"] = json!([
            { "provider": "opencode", "hourStart": "2026-09-22T08:00:00.000Z", "plan": "pro" }
        ]);
        let (status, body) = put(app.clone(), MACHINE, Some(secret), &used).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|error| error.contains("opencode reports no plan hints")),
            "{body}"
        );

        used["providerHints"] = json!([]);
        let (status, body) = put(app.clone(), MACHINE, Some(secret), &used).await;
        assert_eq!(
            (status, body),
            (StatusCode::OK, json!({ "storedBuckets": 1 }))
        );

        // A machine that never ran OpenCode reports it as missing in every sync.
        let mut unused = payload(0);
        unused["coverage"] = coverage("missing");
        unused["providerHints"] = json!([]);
        let other_machine = "0b7a3c52-9d1e-4f6a-8b2c-3e4d5f6a7b8c";
        unused["machine"]["id"] = json!(other_machine);
        let (status, _) = put(app, other_machine, Some(secret), &unused).await;
        assert_eq!(status, StatusCode::OK);

        let stored: Vec<(String, String)> = sqlx::query_as(
            "SELECT 'bucket paid by ' || billing_provider, provider FROM ai_usage_buckets
             UNION ALL SELECT 'hint ' || plan, provider FROM ai_usage_provider_hints
             UNION ALL SELECT 'coverage ' || status, provider FROM ai_usage_machine_coverage
             UNION ALL SELECT 'log', provider FROM ai_usage_coverage_log
             ORDER BY 1",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            [
                "bucket paid by copilot",
                "coverage missing",
                "coverage ok",
                "log"
            ]
            .map(|row| (row.to_string(), "opencode".to_string()))
        );
    }
}
