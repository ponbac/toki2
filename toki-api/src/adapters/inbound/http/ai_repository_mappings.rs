//! Admin-only repository mappings, under
//! `/ai-usage/admin/repository-mappings`: the repositories a metered payer
//! (GitHub Copilot) billed code review credits to, and the time-tracking
//! projects those credits bill to.
//!
//! Every route needs admin power: an admin in a browser session, never an API
//! token (`auth::require_admin_power`). The routes are browser-only, off the
//! agent OpenAPI document and MCP, like the rest of admin billing.
//! Repository names travel in JSON bodies, like project keys: they are free
//! text. Every malformed body is a `400` with the usual JSON error.

use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    http::StatusCode,
    middleware,
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    adapters::inbound::http::{
        ai_metered_usage::credits,
        ai_project_mappings::{AiMappingStatusResponse, AiProjectMappingEditorResponse},
        ai_subscriptions::iso_date,
        ProjectResponse,
    },
    auth::{require_admin_power, AuthUser},
    domain::{
        models::{
            AiMappedProject, AiRepositoryMappingEntry, AiRepositoryMappingOverview,
            AiRepositoryMappingRow, AiRepositorySuggestion, ProjectId,
        },
        ports::inbound::AiRepositoryMappingService,
        AiRepositoryMappingError,
    },
    routes::ApiError,
};

/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiRepositoryMappingService>: FromRef<S>,
{
    Router::new()
        .route(
            "/repository-mappings",
            get(overview).put(set_mapping).delete(delete_mapping),
        )
        .route_layer(middleware::from_fn(require_admin_power))
}

impl From<AiRepositoryMappingError> for ApiError {
    fn from(err: AiRepositoryMappingError) -> Self {
        match err {
            AiRepositoryMappingError::InvalidRepository(message) => Self::bad_request(message),
            AiRepositoryMappingError::UnknownProject(_) => Self::bad_request(err.to_string()),
            AiRepositoryMappingError::UnknownRepository(_) | AiRepositoryMappingError::NotFound => {
                Self::not_found(err.to_string())
            }
            // Reported once at startup; each request only says so.
            AiRepositoryMappingError::NotConfigured => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, err.to_string())
            }
            AiRepositoryMappingError::ProjectsUnavailable(message) => {
                tracing::error!("Time-tracking projects are unavailable: {}", message);
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "time-tracking projects are unavailable",
                )
            }
            AiRepositoryMappingError::Storage(message) => {
                tracing::error!("AI repository mapping operation failed: {}", message);
                Self::internal("ai repository mapping operation failed")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetMappingRequest {
    repository: String,
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteMappingRequest {
    repository: String,
}

/// Every repository the payer billed and every mapping, with credits in the
/// previous and current UTC month.
async fn overview(
    State(service): State<Arc<dyn AiRepositoryMappingService>>,
) -> Result<Json<RepositoryMappingsResponse>, ApiError> {
    Ok(Json(service.overview().await?.into()))
}

/// Maps a repository to an active time-tracking project. A new mapping needs
/// a repository the payer billed; an existing one can always be changed.
async fn set_mapping(
    user: AuthUser,
    State(service): State<Arc<dyn AiRepositoryMappingService>>,
    WithRejection(Json(request), _): JsonBody<SetMappingRequest>,
) -> Result<Json<RepositoryMappingResponse>, ApiError> {
    let mapping = service
        .set_mapping(
            &request.repository,
            &ProjectId::new(request.project_id),
            user.id,
        )
        .await?;
    Ok(Json(mapping.into()))
}

/// Removes a repository's mapping: its credits bill as Unassigned.
async fn delete_mapping(
    State(service): State<Arc<dyn AiRepositoryMappingService>>,
    WithRejection(Json(request), _): JsonBody<DeleteMappingRequest>,
) -> Result<StatusCode, ApiError> {
    service.delete_mapping(&request.repository).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn project(project: AiMappedProject) -> ProjectResponse {
    ProjectResponse {
        project_id: project.id.to_string(),
        project_name: project.name,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryMappingsResponse {
    /// The metered payer, such as `copilot`.
    provider: &'static str,
    /// The metered cutover (`YYYY-MM`), the first month whose bill has
    /// repository lines; null without one.
    billed_from: Option<String>,
    /// The months credits are shown for, oldest first.
    months: Vec<String>,
    /// Those with a billed month (`billed`) that bills as Unassigned first,
    /// then by name, case-insensitively.
    repositories: Vec<RepositoryRowResponse>,
}

impl From<AiRepositoryMappingOverview> for RepositoryMappingsResponse {
    fn from(overview: AiRepositoryMappingOverview) -> Self {
        Self {
            provider: overview.payer.as_str(),
            billed_from: overview.billed_from.map(|month| month.to_string()),
            months: overview.months.iter().map(ToString::to_string).collect(),
            repositories: overview.repositories.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryRowResponse {
    /// The bare repository name, as the payer last spelled it.
    repository: String,
    /// Exactly the overview's months, in order.
    months: Vec<RepositoryMonthResponse>,
    /// The latest UTC day the payer billed it; null for a mapping of a
    /// repository never billed.
    #[serde(with = "iso_date::option")]
    last_billed_on: Option<Date>,
    mapping: Option<RepositoryMappingResponse>,
    /// Projects of resolving project-key mappings naming the repository;
    /// empty when its mapping resolves.
    suggestions: Vec<RepositorySuggestionResponse>,
}

impl From<AiRepositoryMappingRow> for RepositoryRowResponse {
    fn from(row: AiRepositoryMappingRow) -> Self {
        Self {
            repository: row.repository,
            months: row
                .months
                .into_iter()
                .map(|month| RepositoryMonthResponse {
                    month: month.month.to_string(),
                    credits: credits(month.credits),
                    billed: month.billed,
                })
                .collect(),
            last_billed_on: row.last_billed_on,
            mapping: row.mapping.map(Into::into),
            suggestions: row.suggestions.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryMonthResponse {
    month: String,
    /// Hundredths of a credit, `"1200.00"`.
    credits: String,
    /// Whether the month's credits bill a line: the month is on or after the
    /// cutover and its credits come to at least one credit once rounded.
    billed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryMappingResponse {
    /// As spelled when the mapping was last saved.
    repository: String,
    project: ProjectResponse,
    /// `resolves`, `stale` (the project belongs to another company than the
    /// configured one) or `unconfigured`: only a resolving mapping bills the
    /// repository's credits to its project.
    status: AiMappingStatusResponse,
    /// Null once that user is deleted.
    created_by: Option<AiProjectMappingEditorResponse>,
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// Null once that user is deleted.
    updated_by: Option<AiProjectMappingEditorResponse>,
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
}

impl From<AiRepositoryMappingEntry> for RepositoryMappingResponse {
    fn from(entry: AiRepositoryMappingEntry) -> Self {
        let mapping = entry.mapping;
        Self {
            repository: mapping.repository,
            project: project(mapping.project),
            status: entry.resolution.into(),
            created_by: mapping.created_by.map(Into::into),
            created_at: mapping.created_at,
            updated_by: mapping.updated_by.map(Into::into),
            updated_at: mapping.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositorySuggestionResponse {
    project: ProjectResponse,
    /// The mapped project keys naming the repository; never empty.
    project_keys: Vec<String>,
}

impl From<AiRepositorySuggestion> for RepositorySuggestionResponse {
    fn from(suggestion: AiRepositorySuggestion) -> Self {
        Self {
            project: project(suggestion.project),
            project_keys: suggestion.project_keys,
        }
    }
}

#[cfg(test)]
mod tests;
