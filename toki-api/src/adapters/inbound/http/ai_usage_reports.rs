//! The caller's own AI usage, for the personal usage view.
//!
//! Every route reads the authenticated user's usage and nobody else's, for
//! admins too: there is no user parameter. They take a browser session or the
//! user's own API token. Because only the owner reads them, they go down to
//! local hours and sessions. Costs are API-equivalent USD estimates, not bills,
//! and an unknown cost is null, never zero.
//!
//! Every malformed query is a `400` with the usual JSON error.

use std::sync::Arc;

use axum::{
    extract::{FromRef, Query, State},
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};

use crate::{
    adapters::inbound::http::{
        ai_subscriptions::iso_date, ai_usage::AiUsageProvider, ErrorResponse, ProjectResponse,
    },
    auth::AuthUser,
    domain::{
        models::{
            AiCoverageStatus, AiLocalCalendar, AiMachineCoverage, AiMachineHealth, AiMachineId,
            AiMachineList, AiMappedProject, AiPricingStatus, AiProjectAttribution, AiProjectFilter,
            AiProjectKeyList, AiProjectKeyUsage, AiProjectResolution, AiSessionOrder,
            AiUsageFilter, AiUsageReport, AiUsageReportQuery, AiUsageSession, AiUsageSessionList,
            AiUsageTotals, ProjectId, MACHINE_STALE_AFTER,
        },
        ports::inbound::AiUsageReportService,
    },
    routes::ApiError,
};

/// A query string whose rejection is a `400` JSON error.
type QueryParams<T> = WithRejection<Query<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn AiUsageReportService>: FromRef<S>,
{
    Router::new()
        .route("/report", get(get_report))
        .route("/sessions", get(list_sessions))
        .route("/machines", get(list_machines))
        .route("/project-keys", get(list_project_keys))
}

/// The local days and usage to report on. Serde's `flatten` cannot read numbers
/// or booleans from a query string, so the session listing repeats these fields.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AiUsageReportParams {
    /// The first local day, `YYYY-MM-DD`, in the server's AI usage time zone.
    /// Defaults to the first day of `to`'s month.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    from: Option<Date>,
    /// The last local day, inclusive. Defaults to the last day of the current
    /// month. A report covers at most 366 days.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    to: Option<Date>,
    /// Only this provider's usage.
    #[param(inline)]
    provider: Option<AiUsageProvider>,
    /// Only this model's usage; empty for usage whose model is unknown.
    model: Option<String>,
    /// Only usage attributed to this time-tracking project.
    project_id: Option<String>,
    /// Only usage attributed to no project. Not combined with `projectId`.
    #[serde(default)]
    unassigned: bool,
    /// Only this machine's usage.
    #[param(format = "uuid")]
    machine_id: Option<String>,
}

impl AiUsageReportParams {
    fn into_query(self) -> Result<AiUsageReportQuery, ApiError> {
        report_query(
            self.from,
            self.to,
            self.provider,
            self.model,
            self.project_id,
            self.unassigned,
            self.machine_id,
        )
    }
}

/// The sessions to list: the report's range and filters, an order and a limit.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AiUsageSessionParams {
    /// The first local day, `YYYY-MM-DD`. Defaults to the first day of `to`'s month.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    from: Option<Date>,
    /// The last local day, inclusive. Defaults to the last day of the current month.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    to: Option<Date>,
    #[param(inline)]
    provider: Option<AiUsageProvider>,
    model: Option<String>,
    project_id: Option<String>,
    #[serde(default)]
    unassigned: bool,
    #[param(format = "uuid")]
    machine_id: Option<String>,
    /// `recent` (default), `cost` or `tokens`, largest first. By cost, a session
    /// with unpriced usage ranks by its known subtotal.
    #[serde(default)]
    #[param(inline)]
    sort: AiUsageSessionSort,
    /// How many sessions to return, 1 to 1000. Defaults to 100.
    #[param(minimum = 1, maximum = 1000)]
    limit: Option<usize>,
    /// Continue after a previous page: its `nextCursor`, with the same `sort`,
    /// range and filters.
    after: Option<String>,
}

/// The local days whose usage each machine reports.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct AiUsageMachineParams {
    /// The first local day, `YYYY-MM-DD`. Defaults to the first day of `to`'s month.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    from: Option<Date>,
    /// The last local day, inclusive. Defaults to the last day of the current month.
    #[serde(default, with = "iso_date::option")]
    #[param(value_type = Option<String>, format = Date)]
    to: Option<Date>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsageSessionSort {
    #[default]
    Recent,
    Cost,
    Tokens,
}

impl From<AiUsageSessionSort> for AiSessionOrder {
    fn from(sort: AiUsageSessionSort) -> Self {
        match sort {
            AiUsageSessionSort::Recent => Self::Recent,
            AiUsageSessionSort::Cost => Self::Cost,
            AiUsageSessionSort::Tokens => Self::Tokens,
        }
    }
}

fn report_query(
    from: Option<Date>,
    to: Option<Date>,
    provider: Option<AiUsageProvider>,
    model: Option<String>,
    project_id: Option<String>,
    unassigned: bool,
    machine_id: Option<String>,
) -> Result<AiUsageReportQuery, ApiError> {
    let project = match (project_id, unassigned) {
        (Some(_), true) => {
            return Err(ApiError::bad_request(
                "projectId and unassigned cannot be combined",
            ))
        }
        (Some(id), false) => Some(AiProjectFilter::Project(ProjectId::new(id))),
        (None, true) => Some(AiProjectFilter::Unassigned),
        (None, false) => None,
    };
    let machine = machine_id
        .map(|id| {
            AiMachineId::parse(&id).ok_or_else(|| ApiError::bad_request("machineId must be a UUID"))
        })
        .transpose()?;

    Ok(AiUsageReportQuery {
        from,
        to,
        filter: AiUsageFilter {
            provider: provider.map(Into::into),
            model,
            project,
            machine,
        },
    })
}

/// Tokens in four disjoint categories: `input` excludes cache reads and writes,
/// and reasoning is inside `output`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageTokensResponse {
    input: i64,
    cache_read: i64,
    cache_write: i64,
    output: i64,
}

/// Usage summed over some buckets.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageTotalsResponse {
    tokens: AiUsageTokensResponse,
    /// Deduplicated provider requests.
    records: i64,
    /// The API-equivalent cost estimate in USD, or null when any of the usage
    /// could not be priced: then the cost is unknown, not zero.
    estimated_cost_usd: Option<f64>,
    /// The sum of the known costs. It is the whole estimate when
    /// `unpricedRecords` is 0, and only a known subtotal otherwise.
    priced_cost_usd: f64,
    /// Requests without a known price.
    unpriced_records: i64,
}

impl From<AiUsageTotals> for AiUsageTotalsResponse {
    fn from(totals: AiUsageTotals) -> Self {
        Self {
            tokens: AiUsageTokensResponse {
                input: totals.tokens.input,
                cache_read: totals.tokens.cache_read,
                cache_write: totals.tokens.cache_write,
                output: totals.tokens.output,
            },
            records: totals.records,
            estimated_cost_usd: totals.estimated_cost_usd(),
            priced_cost_usd: totals.priced_cost_usd,
            unpriced_records: totals.unpriced_records,
        }
    }
}

/// How usage of a project key is attributed.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiProjectAttributionStatus {
    /// Mapped to a time-tracking project, named in `project`.
    Mapped,
    /// Mapped to a project of another provider or company, so unassigned until
    /// an admin maps the key again.
    Stale,
    /// Mapped, but time tracking is not configured on the server, so no mapping
    /// resolves. Mapping again does not help; configuring time tracking does.
    Unconfigured,
    /// Not mapped yet; see `mappable`.
    Unmapped,
    /// Usage without project metadata, which cannot be mapped. token-ledger
    /// attributes usage to a configured project name or a Git remote.
    Unattributed,
}

/// A project key and how its usage is attributed. Usage that is not `mapped`
/// is unassigned.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiProjectAttributionResponse {
    /// A configured project name, `host/owner/repo`, `owner/repo`, or `unattributed`.
    project_key: String,
    status: AiProjectAttributionStatus,
    /// The project the usage counts for; null unless `status` is `mapped`.
    project: Option<ProjectResponse>,
    /// Whether you can map the key: it is unmapped, has no surrounding
    /// whitespace, and time tracking is configured. Changing an existing mapping
    /// takes an admin.
    mappable: bool,
}

impl From<AiProjectAttribution> for AiProjectAttributionResponse {
    fn from(attribution: AiProjectAttribution) -> Self {
        let mappable = attribution.mappable();
        let (status, project) = match attribution.resolution {
            AiProjectResolution::Mapped(project) => (
                AiProjectAttributionStatus::Mapped,
                Some(project_response(project)),
            ),
            AiProjectResolution::Stale => (AiProjectAttributionStatus::Stale, None),
            AiProjectResolution::Unconfigured => (AiProjectAttributionStatus::Unconfigured, None),
            AiProjectResolution::Unmapped { .. } => (AiProjectAttributionStatus::Unmapped, None),
            AiProjectResolution::Unattributed => (AiProjectAttributionStatus::Unattributed, None),
        };

        Self {
            project_key: attribution.project_key,
            status,
            project,
            mappable,
        }
    }
}

fn project_response(project: AiMappedProject) -> ProjectResponse {
    ProjectResponse {
        project_id: project.id.to_string(),
        project_name: project.name,
    }
}

/// Your AI usage in a range of local days, narrowed by the filters.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageReportResponse {
    /// The IANA time zone whose calendar days and hours the report uses, such
    /// as `Europe/Stockholm`.
    time_zone: String,
    /// Today's date in that time zone, `YYYY-MM-DD`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    today: Date,
    /// The first local day covered.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    from: Date,
    /// The last local day covered, inclusive.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    to: Date,
    /// Whether time tracking is configured on the server. Without it no usage
    /// counts for a project, every mapped key is `unconfigured`, and no key can
    /// be mapped.
    time_tracking_configured: bool,
    options: AiUsageFilterOptionsResponse,
    totals: AiUsageTotalsResponse,
    /// Per local day, provider and model, by day. Days without usage are absent.
    days: Vec<AiUsageDayResponse>,
    /// Per project, costliest first. All unassigned usage is one entry whose
    /// `project` is null.
    projects: Vec<AiUsageProjectResponse>,
    /// Per provider and model, costliest first.
    models: Vec<AiUsageModelResponse>,
    /// Per machine, costliest first, narrowed by the filters like every other
    /// figure. `/ai-usage/machines` reports each machine's unfiltered usage.
    machines: Vec<AiUsageMachineUsageResponse>,
    /// Per local weekday and hour of day. Hours without usage are absent.
    hours: Vec<AiUsageHourResponse>,
}

/// The values the filters can take: those with usage in the range, whichever
/// filters are applied.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageFilterOptionsResponse {
    providers: Vec<AiUsageProvider>,
    /// Costliest first.
    models: Vec<String>,
    /// By name.
    projects: Vec<ProjectResponse>,
    /// Whether any usage in the range is unassigned.
    unassigned: bool,
    #[schema(format = "uuid")]
    machine_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageDayResponse {
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    date: Date,
    provider: AiUsageProvider,
    /// Empty when the model is unknown.
    model: String,
    totals: AiUsageTotalsResponse,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProjectResponse {
    /// Null for all unassigned usage.
    project: Option<ProjectResponse>,
    totals: AiUsageTotalsResponse,
    /// The project keys whose usage counts for it, costliest first.
    keys: Vec<AiUsageProjectKeyTotalsResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProjectKeyTotalsResponse {
    #[serde(flatten)]
    attribution: AiProjectAttributionResponse,
    totals: AiUsageTotalsResponse,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageModelResponse {
    provider: AiUsageProvider,
    /// Empty when the model is unknown.
    model: String,
    totals: AiUsageTotalsResponse,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageMachineUsageResponse {
    #[schema(format = "uuid")]
    machine_id: String,
    totals: AiUsageTotalsResponse,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageHourResponse {
    /// ISO weekday: 1 is Monday, 7 is Sunday.
    #[schema(minimum = 1, maximum = 7)]
    weekday: u8,
    /// The local hour of day the usage started in. When clocks go back, both
    /// runs of the repeated hour count here.
    #[schema(minimum = 0, maximum = 23)]
    hour: u8,
    totals: AiUsageTotalsResponse,
}

impl From<AiUsageReport> for AiUsageReportResponse {
    fn from(report: AiUsageReport) -> Self {
        let AiLocalCalendar { time_zone, today } = report.calendar;
        let options = report.options;

        Self {
            time_zone: time_zone.as_str().to_string(),
            today,
            from: report.dates.start(),
            to: report.dates.last_day(),
            time_tracking_configured: report.time_tracking_configured,
            options: AiUsageFilterOptionsResponse {
                providers: options.providers.into_iter().map(Into::into).collect(),
                models: options.models,
                projects: options.projects.into_iter().map(project_response).collect(),
                unassigned: options.unassigned,
                machine_ids: options.machines.iter().map(ToString::to_string).collect(),
            },
            totals: report.totals.into(),
            days: report
                .days
                .into_iter()
                .map(|day| AiUsageDayResponse {
                    date: day.date,
                    provider: day.provider.into(),
                    model: day.model,
                    totals: day.totals.into(),
                })
                .collect(),
            projects: report
                .projects
                .into_iter()
                .map(|group| AiUsageProjectResponse {
                    project: group.project.map(project_response),
                    totals: group.totals.into(),
                    keys: group
                        .keys
                        .into_iter()
                        .map(|key| AiUsageProjectKeyTotalsResponse {
                            attribution: key.project.into(),
                            totals: key.totals.into(),
                        })
                        .collect(),
                })
                .collect(),
            models: report
                .models
                .into_iter()
                .map(|model| AiUsageModelResponse {
                    provider: model.provider.into(),
                    model: model.model,
                    totals: model.totals.into(),
                })
                .collect(),
            machines: report
                .machines
                .into_iter()
                .map(|machine| AiUsageMachineUsageResponse {
                    machine_id: machine.machine_id.to_string(),
                    totals: machine.totals.into(),
                })
                .collect(),
            hours: report
                .hours
                .into_iter()
                .map(|hour| AiUsageHourResponse {
                    weekday: hour.weekday.number_from_monday(),
                    hour: hour.hour,
                    totals: hour.totals.into(),
                })
                .collect(),
        }
    }
}

/// Your sessions in a range, as far as their usage lies in it.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageSessionListResponse {
    /// The IANA time zone of `from` and `to`, for showing the UTC hours locally.
    time_zone: String,
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    from: Date,
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    to: Date,
    /// How many sessions the range and filters hold, listed or not.
    total: usize,
    sessions: Vec<AiUsageSessionResponse>,
    /// Pass as `after`, with the same `sort`, range and filters, for the next
    /// page; null after the last page.
    next_cursor: Option<String>,
}

/// One session of one tool on one machine.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageSessionResponse {
    /// Opaque and stable per session.
    session_key: String,
    #[schema(format = "uuid")]
    machine_id: String,
    provider: AiUsageProvider,
    /// The project keys its usage was attributed to.
    projects: Vec<AiProjectAttributionResponse>,
    /// Model names; empty for an unknown model.
    models: Vec<String>,
    /// The start of the first hour with usage, a UTC instant.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    first_active_hour: OffsetDateTime,
    /// The start of the last hour with usage, a UTC instant.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    last_active_hour: OffsetDateTime,
    totals: AiUsageTotalsResponse,
}

impl From<AiUsageSession> for AiUsageSessionResponse {
    fn from(session: AiUsageSession) -> Self {
        Self {
            session_key: session.session_key,
            machine_id: session.machine_id.to_string(),
            provider: session.provider.into(),
            projects: session.projects.into_iter().map(Into::into).collect(),
            models: session.models,
            first_active_hour: session.first_hour,
            last_active_hour: session.last_hour,
            totals: session.totals.into(),
        }
    }
}

impl From<AiUsageSessionList> for AiUsageSessionListResponse {
    fn from(list: AiUsageSessionList) -> Self {
        Self {
            time_zone: list.calendar.time_zone.as_str().to_string(),
            from: list.dates.start(),
            to: list.dates.last_day(),
            total: list.total,
            sessions: list.sessions.into_iter().map(Into::into).collect(),
            next_cursor: list.next.map(|cursor| cursor.encode()),
        }
    }
}

/// Your machines and what each last reported.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageMachineListResponse {
    /// A machine is stale after this many days without a sync.
    stale_after_days: i64,
    /// The first local day of each machine's `usage`.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    from: Date,
    /// The last local day of each machine's `usage`, inclusive.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    to: Date,
    /// Most recently synced first.
    machines: Vec<AiUsageMachineResponse>,
}

/// A machine that uploads your usage with `token-ledger sync`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageMachineResponse {
    #[schema(format = "uuid")]
    machine_id: String,
    label: String,
    /// The token-ledger version of the last sync.
    client_version: String,
    /// The machine's IANA time zone, for display only.
    time_zone: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    first_seen_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    last_synced_at: OffsetDateTime,
    /// No sync for more than `staleAfterDays`: its usage since is missing, not zero.
    stale: bool,
    /// The coverage each provider last reported, by provider.
    coverage: Vec<AiUsageCoverageResponse>,
    /// All of the machine's usage from `from` to `to`, or null without any.
    usage: Option<AiUsageTotalsResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsageCoverageStatusResponse {
    Ok,
    Partial,
    Failed,
    /// No history was found.
    Missing,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiUsagePricingStatusResponse {
    Fresh,
    Cached,
    Unavailable,
    Custom,
}

/// How completely the machine could read one provider's history last time.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageCoverageResponse {
    provider: AiUsageProvider,
    status: AiUsageCoverageStatusResponse,
    files: i64,
    /// Existing history paths that could not be read.
    unreadable: i64,
    malformed_lines: i64,
    skipped_records: i64,
    duplicates: i64,
    /// The window of the upload that reported it.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    window_start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    window_end: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    reported_at: OffsetDateTime,
    /// The rates behind the stored costs, or null until an upload first
    /// replaced the provider's usage.
    pricing: Option<AiUsagePricingResponse>,
    /// Whether usage of the provider from this machine is stored.
    has_usage: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsagePricingResponse {
    status: AiUsagePricingStatusResponse,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    fetched_at: Option<OffsetDateTime>,
    source: String,
}

impl From<AiMachineCoverage> for AiUsageCoverageResponse {
    fn from(entry: AiMachineCoverage) -> Self {
        let coverage = entry.coverage;
        Self {
            provider: coverage.provider.into(),
            status: match coverage.status {
                AiCoverageStatus::Ok => AiUsageCoverageStatusResponse::Ok,
                AiCoverageStatus::Partial => AiUsageCoverageStatusResponse::Partial,
                AiCoverageStatus::Failed => AiUsageCoverageStatusResponse::Failed,
                AiCoverageStatus::Missing => AiUsageCoverageStatusResponse::Missing,
            },
            files: coverage.files,
            unreadable: coverage.unreadable,
            malformed_lines: coverage.malformed_lines,
            skipped_records: coverage.skipped_records,
            duplicates: coverage.duplicates,
            window_start: entry.window_start,
            window_end: entry.window_end,
            reported_at: entry.reported_at,
            pricing: entry.pricing.map(|pricing| AiUsagePricingResponse {
                status: match pricing.status {
                    AiPricingStatus::Fresh => AiUsagePricingStatusResponse::Fresh,
                    AiPricingStatus::Cached => AiUsagePricingStatusResponse::Cached,
                    AiPricingStatus::Unavailable => AiUsagePricingStatusResponse::Unavailable,
                    AiPricingStatus::Custom => AiUsagePricingStatusResponse::Custom,
                },
                fetched_at: pricing.fetched_at,
                source: pricing.source,
            }),
            has_usage: entry.has_usage,
        }
    }
}

impl From<AiMachineHealth> for AiUsageMachineResponse {
    fn from(health: AiMachineHealth) -> Self {
        let status = health.status;
        Self {
            machine_id: status.machine.id.to_string(),
            label: status.machine.label,
            client_version: status.machine.client_version,
            time_zone: status.machine.time_zone,
            first_seen_at: status.first_seen_at,
            last_synced_at: status.last_synced_at,
            stale: health.stale,
            coverage: status.coverage.into_iter().map(Into::into).collect(),
            usage: health.usage.map(Into::into),
        }
    }
}

/// The project keys in your usage.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProjectKeyListResponse {
    /// Whether time tracking is configured on the server. Without it no key
    /// resolves to a project and no key can be mapped.
    time_tracking_configured: bool,
    /// Most recently used first.
    project_keys: Vec<AiUsageProjectKeyResponse>,
}

impl From<AiProjectKeyList> for AiUsageProjectKeyListResponse {
    fn from(list: AiProjectKeyList) -> Self {
        Self {
            time_tracking_configured: list.time_tracking_configured,
            project_keys: list.keys.into_iter().map(Into::into).collect(),
        }
    }
}

/// A project key in your usage and how it is attributed.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageProjectKeyResponse {
    #[serde(flatten)]
    attribution: AiProjectAttributionResponse,
    /// The latest local day with usage of the key, `YYYY-MM-DD`, in the
    /// server's AI usage time zone.
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    last_used_on: Date,
}

impl From<AiProjectKeyUsage> for AiUsageProjectKeyResponse {
    fn from(key: AiProjectKeyUsage) -> Self {
        Self {
            attribution: key.project.into(),
            last_used_on: key.last_used_on,
        }
    }
}

/// Get your AI usage report
///
/// Reports your own AI usage, as uploaded by `token-ledger sync`, for a range
/// of local days in the server's AI usage time zone: totals, and breakdowns by
/// day, project, model, machine and local hour of the week. It defaults to the
/// current month. Filters narrow every figure; `options` lists what they can
/// select. Costs are API-equivalent USD estimates, not bills. When any usage is
/// unpriced, `estimatedCostUsd` is null and `pricedCostUsd` is only the known
/// subtotal.
#[utoipa::path(
    get,
    path = "/ai-usage/report",
    operation_id = "getAiUsageReport",
    tag = "AI usage",
    params(AiUsageReportParams),
    responses(
        (status = 200, description = "Your usage in the range", body = AiUsageReportResponse),
        (status = 400, description = "A query parameter is not valid, or the range is reversed or longer than 366 days", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Usage could not be read", body = ErrorResponse)
    )
)]
pub async fn get_report(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageReportService>>,
    WithRejection(Query(params), _): QueryParams<AiUsageReportParams>,
) -> Result<Json<AiUsageReportResponse>, ApiError> {
    let report = service.report(&user.id, &params.into_query()?).await?;

    Ok(Json(report.into()))
}

/// List your AI usage sessions
///
/// Lists your sessions with usage in a range of local days, with the same
/// range defaults and filters as the report: the projects and models each used,
/// its first and last active hour, tokens and estimated cost. A session counts
/// only its usage inside the range that passes the filters. Sort by `cost` to
/// find expensive sessions; pass `nextCursor` as `after` for the next page.
#[utoipa::path(
    get,
    path = "/ai-usage/sessions",
    operation_id = "listAiUsageSessions",
    tag = "AI usage",
    params(AiUsageSessionParams),
    responses(
        (status = 200, description = "Your sessions in the range", body = AiUsageSessionListResponse),
        (status = 400, description = "A query parameter is not valid, or the range is reversed or longer than 366 days", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Sessions could not be read", body = ErrorResponse)
    )
)]
pub async fn list_sessions(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageReportService>>,
    WithRejection(Query(params), _): QueryParams<AiUsageSessionParams>,
) -> Result<Json<AiUsageSessionListResponse>, ApiError> {
    let query = report_query(
        params.from,
        params.to,
        params.provider,
        params.model,
        params.project_id,
        params.unassigned,
        params.machine_id,
    )?;
    let sessions = service
        .sessions(
            &user.id,
            &query,
            params.sort.into(),
            params.after.as_deref(),
            params.limit,
        )
        .await?;

    Ok(Json(sessions.into()))
}

/// List your AI usage machines
///
/// Lists the machines that upload your usage with `token-ledger sync`: when each
/// last synced, its token-ledger version, how completely it could read each
/// provider's history, and all its usage in a range of local days (by default
/// the current month), which no report filter narrows. A stale machine has not
/// synced for more than `staleAfterDays`, so its recent usage is missing, not
/// zero.
#[utoipa::path(
    get,
    path = "/ai-usage/machines",
    operation_id = "listAiUsageMachines",
    tag = "AI usage",
    params(AiUsageMachineParams),
    responses(
        (status = 200, description = "Your machines", body = AiUsageMachineListResponse),
        (status = 400, description = "`from` or `to` is not a date, or the range is reversed or longer than 366 days", body = ErrorResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Machines could not be read", body = ErrorResponse)
    )
)]
pub async fn list_machines(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageReportService>>,
    WithRejection(Query(params), _): QueryParams<AiUsageMachineParams>,
) -> Result<Json<AiUsageMachineListResponse>, ApiError> {
    let query = AiUsageReportQuery {
        from: params.from,
        to: params.to,
        filter: AiUsageFilter::default(),
    };
    let AiMachineList { dates, machines } = service.machines(&user.id, &query).await?;

    Ok(Json(AiUsageMachineListResponse {
        stale_after_days: MACHINE_STALE_AFTER.whole_days(),
        from: dates.start(),
        to: dates.last_day(),
        machines: machines.into_iter().map(Into::into).collect(),
    }))
}

/// List your AI usage project keys
///
/// Lists every project key in your usage, most recently used first, and how
/// its usage is attributed: to a time-tracking project, or unassigned because
/// the key is unmapped, mapped to another company's project (stale), mapped
/// while time tracking is not configured (unconfigured), or `unattributed`. You
/// can map an unmapped key yourself while time tracking is configured; changing
/// an existing mapping takes an admin.
#[utoipa::path(
    get,
    path = "/ai-usage/project-keys",
    operation_id = "listAiUsageProjectKeys",
    tag = "AI usage",
    responses(
        (status = 200, description = "Your project keys", body = AiUsageProjectKeyListResponse),
        (status = 401, description = "Missing or invalid credentials"),
        (status = 500, description = "Project keys could not be read", body = ErrorResponse)
    )
)]
pub async fn list_project_keys(
    user: AuthUser,
    State(service): State<Arc<dyn AiUsageReportService>>,
) -> Result<Json<AiUsageProjectKeyListResponse>, ApiError> {
    let keys = service.project_keys(&user.id).await?;

    Ok(Json(keys.into()))
}

#[cfg(test)]
mod tests;
