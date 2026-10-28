//! Reprice-only uploads from `token-ledger sync`. When the client's pricing
//! changes, it asks which of a machine's stored usage another pricing basis
//! priced, then sends new costs for the buckets it can still reproduce. Toki
//! updates only buckets that match exactly and never inserts or deletes usage,
//! so history the machine no longer has keeps its stored cost.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};

use super::{is_json, utc_instant, AiUsageBucketPayload, AiUsagePricingStatus, AiUsageProvider};
use crate::{
    adapters::inbound::http::ErrorResponse,
    auth::AuthUser,
    domain::{
        models::{
            AiMachineId, AiPricing, AiPricingBasis, AiProviderRepricing, AiRepricing,
            AiRepricingReceipt, AiRepricingStatus, MAX_PRICING_BASIS_CHARS,
        },
        ports::inbound::AiUsageService,
    },
    routes::ApiError,
};

const SUPPORTED_VERSION: u64 = 1;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AiUsageRepricingParams {
    /// The pricing basis the client prices with now, 1 to 128 characters:
    /// what it sends as `pricing.basis`.
    #[param(min_length = 1, max_length = 128)]
    basis: String,
}

/// Which of the machine's stored usage another pricing basis priced.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageRepricingResponse {
    /// The earliest hour that can be repriced, a UTC instant. Costs before it
    /// stay as they are.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    floor: OffsetDateTime,
    /// One entry per provider with stored usage on the machine, ordered by
    /// provider name. Empty for a machine you have not registered.
    providers: Vec<AiUsageProviderRepricingResponse>,
}

/// One provider's stored usage on the machine, from the floor on, that another
/// pricing basis priced.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProviderRepricingResponse {
    provider: AiUsageProvider,
    /// The earliest such hour, a UTC instant, or null when none is.
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime, required = true)]
    from: Option<OffsetDateTime>,
    /// How many such buckets are stored. Buckets stored without a basis count.
    stale_buckets: u64,
}

/// New costs for stored usage. All instants are RFC 3339 UTC.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageRepricingRequest {
    /// Payload format version; only `1` is supported.
    #[schema(minimum = 1, maximum = 1)]
    version: u64,
    /// token-ledger version, for diagnostics.
    #[schema(min_length = 1, max_length = 512)]
    client_version: String,
    pricing: AiUsageRepricingPricingPayload,
    /// At most 20,000 buckets, each as in an upload with version 3 costs, with
    /// unique keys `(hourStart, sessionKey, project, provider, model,
    /// billingProvider)`. Hours need not lie in any window.
    buckets: Vec<AiUsageBucketPayload>,
}

/// Provenance of the rates behind the new costs, and their pricing basis.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageRepricingPricingPayload {
    status: AiUsagePricingStatus,
    /// When the rates were fetched, or null.
    #[serde(deserialize_with = "utc_instant::nullable")]
    #[schema(value_type = Option<String>, format = DateTime, required = true)]
    fetched_at: Option<OffsetDateTime>,
    #[schema(max_length = 512)]
    source: String,
    /// The pricing basis of every cost in `buckets`, 1 to 128 characters,
    /// stored on each bucket the repricing updates.
    #[schema(min_length = 1, max_length = 128)]
    basis: String,
}

/// What the repricing changed. Every bucket of the request is counted once.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageRepricingReceiptResponse {
    /// Stored buckets that took the new cost and basis, also when they were
    /// equal already.
    updated: u64,
    /// Buckets from the floor on that match no stored bucket exactly: other
    /// token or record counts, another payer, or usage Toki does not store.
    /// Nothing changed for them.
    unmatched: u64,
    /// Buckets before the floor, which are never repriced.
    before_floor: u64,
}

/// Find AI usage to reprice for a machine
///
/// Compares the machine's stored AI usage with the pricing basis the client
/// prices with now. For each provider with stored usage, returns the earliest
/// hour from the reprice floor on whose stored cost another basis priced, and
/// how many such buckets there are. Usage stored without a basis counts. A
/// machine you have not registered has no providers.
#[utoipa::path(
    get,
    path = "/ai-usage/machines/{machine_id}/repricing",
    operation_id = "getAiUsageRepricing",
    tag = "AI usage",
    params(
        ("machine_id" = String, Path, format = "uuid", description = "The machine ID"),
        AiUsageRepricingParams
    ),
    responses(
        (status = 200, description = "Stored usage to reprice, per provider", body = AiUsageRepricingResponse),
        (status = 400, description = "The machine ID is not a UUID, or basis is missing, empty or longer than 128 characters", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Stored usage could not be read", body = ErrorResponse)
    )
)]
pub async fn get_repricing(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageService>>,
    Path(machine_id): Path<String>,
    WithRejection(Query(params), _): WithRejection<Query<AiUsageRepricingParams>, ApiError>,
) -> Result<Json<AiUsageRepricingResponse>, ApiError> {
    let machine_id = machine_id_from_path(&machine_id)?;
    let basis = AiPricingBasis::parse(params.basis).ok_or_else(|| {
        ApiError::bad_request(format!(
            "basis must be 1 to {MAX_PRICING_BASIS_CHARS} characters without NUL"
        ))
    })?;

    let status = service
        .repricing_status(&user.id, &machine_id, &basis)
        .await?;

    Ok(Json(status.into()))
}

/// Reprice stored AI usage for a machine
///
/// Gives stored AI usage of the machine new costs, without replacing any
/// usage. A bucket from the reprice floor on takes its new cost, unpriced
/// record count and pricing basis only when a stored bucket has the same key,
/// the same four token counts and the same record count; Copilot's next edit
/// suggestions are stored at no cost, as in an upload. The request is applied
/// in one transaction. It never inserts or deletes usage and changes no
/// coverage, hints, snapshot or sync time. Buckets of a machine you have not
/// registered match nothing.
#[utoipa::path(
    post,
    path = "/ai-usage/machines/{machine_id}/repricing",
    operation_id = "repriceAiUsage",
    tag = "AI usage",
    params(
        ("machine_id" = String, Path, format = "uuid", description = "The machine ID")
    ),
    request_body = AiUsageRepricingRequest,
    responses(
        (status = 200, description = "Matching stored usage repriced", body = AiUsageRepricingReceiptResponse),
        (status = 400, description = "The body is not a valid version 1 payload, repeats a bucket key or has more than 20,000 buckets", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 413, description = "The body exceeds 16 MiB"),
        (status = 415, description = "The body is not JSON"),
        (status = 500, description = "Usage could not be repriced", body = ErrorResponse)
    )
)]
pub async fn reprice_usage(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageService>>,
    Path(machine_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AiUsageRepricingReceiptResponse>, ApiError> {
    if !is_json(&headers) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected an application/json body",
        ));
    }
    let request = serde_json::from_slice::<AiUsageRepricingRequest>(&body)
        .map_err(|error| ApiError::bad_request(format!("invalid repricing payload: {error}")))?;
    let machine_id = machine_id_from_path(&machine_id)?;
    let repricing = request.into_repricing(machine_id)?;

    let receipt = service.reprice(&user.id, &repricing).await?;

    Ok(Json(receipt.into()))
}

fn machine_id_from_path(raw: &str) -> Result<AiMachineId, ApiError> {
    AiMachineId::parse(raw).ok_or_else(|| ApiError::bad_request("machine_id must be a UUID"))
}

impl AiUsageRepricingRequest {
    fn into_repricing(self, machine_id: AiMachineId) -> Result<AiRepricing, ApiError> {
        if self.version != SUPPORTED_VERSION {
            return Err(ApiError::bad_request(format!(
                "unsupported repricing payload version {}; only version {SUPPORTED_VERSION} is supported",
                self.version
            )));
        }
        let basis = super::pricing_basis(self.pricing.basis)?;
        let pricing = AiPricing {
            status: self.pricing.status.into(),
            fetched_at: self.pricing.fetched_at,
            source: self.pricing.source,
        };

        Ok(AiRepricing::new(
            machine_id,
            self.client_version,
            pricing,
            basis,
            self.buckets.into_iter().map(Into::into).collect(),
        )?)
    }
}

impl From<AiRepricingStatus> for AiUsageRepricingResponse {
    fn from(status: AiRepricingStatus) -> Self {
        Self {
            floor: status.floor,
            providers: status.providers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<AiProviderRepricing> for AiUsageProviderRepricingResponse {
    fn from(entry: AiProviderRepricing) -> Self {
        Self {
            provider: entry.provider.into(),
            from: entry.from,
            stale_buckets: entry.stale_buckets,
        }
    }
}

impl From<AiRepricingReceipt> for AiUsageRepricingReceiptResponse {
    fn from(receipt: AiRepricingReceipt) -> Self {
        Self {
            updated: receipt.updated,
            unmatched: receipt.unmatched,
            before_floor: receipt.before_floor,
        }
    }
}

#[cfg(test)]
mod tests;
