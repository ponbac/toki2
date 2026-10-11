//! A developer's own AI usage, read back for the personal usage view.
//!
//! Every read here covers one user's usage and nobody else's. Because only the
//! owner reads it, it goes down to local hours of day and sessions; views of
//! other developers' usage aggregate to days instead. Costs are API-equivalent
//! USD estimates, not bills. The store sums them exactly, as NUMERIC, and
//! converts each sum once; a cost that is unknown for any usage stays unknown:
//! totals keep the known subtotal and count what is unpriced.

use std::collections::{BTreeMap, HashMap};

use time::{Date, Duration, OffsetDateTime, Weekday};

use super::{
    AiLocalCalendar, AiMachine, AiMachineId, AiMappedProject, AiMappingResolution,
    AiMappingResolver, AiPricing, AiProjectKey, AiProvider, AiProviderCoverage, AiTokenCounts,
    AiUsageDateRange, AiUsageTotals, ProjectId, UNATTRIBUTED_PROJECT_KEY,
};
use crate::domain::AiUsageError;

/// The longest range one report covers, in local days.
pub const MAX_REPORT_DAYS: i64 = 366;
/// A machine that has not synced for longer than this is stale: its usage since
/// its last sync is missing, not zero.
pub const MACHINE_STALE_AFTER: Duration = Duration::days(7);
/// How many sessions a listing returns unless asked for another number.
pub const DEFAULT_SESSIONS_LIMIT: usize = 100;
/// The most sessions one listing returns.
pub const MAX_SESSIONS_LIMIT: usize = 1_000;

impl AiTokenCounts {
    /// Every token, across the four disjoint categories.
    pub fn total(&self) -> i64 {
        self.input + self.cache_read + self.cache_write + self.output
    }
}

/// How usage of a project key is attributed when it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProjectResolution {
    /// Mapped to a project of the configured time-tracking company.
    Mapped(AiMappedProject),
    /// Mapped to a project of another provider or company, so unassigned until
    /// an admin maps the key again.
    Stale,
    /// Mapped, but time tracking is not configured, so no mapping resolves.
    /// Only configuring time tracking changes that; mapping again does not.
    Unconfigured,
    /// Not mapped yet. `mappable` when the key can be mapped: it is a valid key
    /// and time tracking is configured.
    Unmapped { mappable: bool },
    /// Usage without project metadata, which can never be mapped.
    Unattributed,
}

/// A project key and how its usage is attributed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectAttribution {
    pub project_key: String,
    pub resolution: AiProjectResolution,
}

impl AiProjectAttribution {
    /// Attributes a key through the stored mappings.
    pub fn of(project_key: String, resolver: &AiMappingResolver) -> Self {
        let resolution = if project_key == UNATTRIBUTED_PROJECT_KEY {
            AiProjectResolution::Unattributed
        } else {
            match resolver.mapping(&project_key) {
                Some((mapping, AiMappingResolution::Resolves)) => {
                    AiProjectResolution::Mapped(mapping.project.clone())
                }
                Some((_, AiMappingResolution::Stale)) => AiProjectResolution::Stale,
                Some((_, AiMappingResolution::Unconfigured)) => AiProjectResolution::Unconfigured,
                None => AiProjectResolution::Unmapped {
                    mappable: resolver.is_configured() && AiProjectKey::parse(&project_key).is_ok(),
                },
            }
        };

        Self {
            project_key,
            resolution,
        }
    }

    /// The project the usage counts for, or `None` when it is unassigned.
    pub fn project(&self) -> Option<&AiMappedProject> {
        match &self.resolution {
            AiProjectResolution::Mapped(project) => Some(project),
            _ => None,
        }
    }

    /// Whether a developer with usage of the key may map it. Changing an
    /// existing mapping is admin-only.
    pub fn mappable(&self) -> bool {
        matches!(
            self.resolution,
            AiProjectResolution::Unmapped { mappable: true }
        )
    }
}

/// Which usage a project filter keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiProjectFilter {
    /// Usage attributed to the project, through any of its keys.
    Project(ProjectId),
    /// Usage attributed to no project: unmapped, stale, unconfigured and
    /// unattributed keys.
    Unassigned,
}

/// Narrows a report to some of the usage in its range. Every figure of a
/// report respects every filter; its filter options do not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiUsageFilter {
    pub provider: Option<AiProvider>,
    /// An exact model name; empty for usage whose model is unknown.
    pub model: Option<String>,
    pub project: Option<AiProjectFilter>,
    pub machine: Option<AiMachineId>,
}

/// The local dates and usage a personal report covers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiUsageReportQuery {
    /// The first local date, inclusive. Defaults to the first day of `to`'s month.
    pub from: Option<Date>,
    /// The last local date, inclusive. Defaults to the last day of the current
    /// month in the configured time zone.
    pub to: Option<Date>,
    pub filter: AiUsageFilter,
}

/// Which project keys a selection keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiKeySelection {
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

/// A filter as the store applies it: the project filter becomes a set of keys
/// through the mappings, so the store never decides attribution itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiUsageSelection {
    pub provider: Option<AiProvider>,
    pub model: Option<String>,
    pub machine: Option<AiMachineId>,
    pub keys: AiKeySelection,
    /// Every key whose usage counts for a project, with the project, so the
    /// store can sum per project.
    pub projects: Vec<(String, ProjectId)>,
}

impl AiUsageSelection {
    pub fn new(filter: &AiUsageFilter, resolver: &AiMappingResolver) -> Self {
        let projects = resolver
            .resolved_keys()
            .into_iter()
            .map(|(key, project)| (key.to_string(), project.id.clone()))
            .collect::<Vec<_>>();
        let keys = match &filter.project {
            None => AiKeySelection::All,
            Some(AiProjectFilter::Project(id)) => AiKeySelection::Only(
                projects
                    .iter()
                    .filter(|(_, project)| project == id)
                    .map(|(key, _)| key.clone())
                    .collect(),
            ),
            Some(AiProjectFilter::Unassigned) => {
                AiKeySelection::Except(projects.iter().map(|(key, _)| key.clone()).collect())
            }
        };

        Self {
            provider: filter.provider,
            model: filter.model.clone(),
            machine: filter.machine,
            keys,
            projects,
        }
    }
}

/// The local dates a report covers, from inclusive `from` and `to` dates.
/// Without `to`, the range ends on the last day of the current month; without
/// `from`, it starts on the first day of `to`'s month. So by default a report
/// covers the current month in the configured time zone.
pub fn report_dates(
    today: Date,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<AiUsageDateRange, AiUsageError> {
    let out_of_range = || AiUsageError::InvalidQuery("from or to is out of range".to_string());
    let to = match to {
        Some(to) => to,
        None => today
            .replace_day(today.month().length(today.year()))
            .map_err(|_| out_of_range())?,
    };
    let from = match from {
        Some(from) => from,
        None => to.replace_day(1).map_err(|_| out_of_range())?,
    };
    if from > to {
        return Err(AiUsageError::InvalidQuery(
            "from must not be after to".to_string(),
        ));
    }

    let dates = AiUsageDateRange::from_inclusive(from, to).ok_or_else(out_of_range)?;
    if (dates.end() - dates.start()).whole_days() > MAX_REPORT_DAYS {
        return Err(AiUsageError::InvalidQuery(format!(
            "a report covers at most {MAX_REPORT_DAYS} days"
        )));
    }

    Ok(dates)
}

/// A selection's usage summed by the store per breakdown. Every sum covers its
/// buckets exactly once, so no figure is a sum of sums.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiUsageSums {
    pub totals: AiUsageTotals,
    pub days: Vec<AiUsageDayTotals>,
    /// Per project the selection's `projects` resolve to; `None` sums all
    /// unassigned usage.
    pub projects: Vec<(Option<ProjectId>, AiUsageTotals)>,
    pub keys: Vec<(String, AiUsageTotals)>,
    pub models: Vec<AiUsageModelTotals>,
    pub machines: Vec<AiUsageMachineTotals>,
    pub hours: Vec<AiUsageHourOfWeekTotals>,
}

/// What a range holds, whatever the filters: the values they can select.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiUsageOptionSums {
    pub providers: Vec<AiProvider>,
    pub models: Vec<(String, AiUsageTotals)>,
    pub project_keys: Vec<String>,
    pub machines: Vec<AiMachineId>,
}

/// A developer's usage in a range of local dates, narrowed by a filter.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageReport {
    pub calendar: AiLocalCalendar,
    pub dates: AiUsageDateRange,
    /// Whether time tracking is configured. Without it no usage counts for a
    /// project and nothing can be mapped.
    pub time_tracking_configured: bool,
    /// What the filters can select in the range, whichever filters are applied.
    pub options: AiUsageFilterOptions,
    pub totals: AiUsageTotals,
    /// Per local day, provider and model, by day.
    pub days: Vec<AiUsageDayTotals>,
    /// Per project, with all unassigned usage as one group, costliest first.
    pub projects: Vec<AiUsageProjectTotals>,
    /// Per provider and model, costliest first.
    pub models: Vec<AiUsageModelTotals>,
    /// Per machine, costliest first.
    pub machines: Vec<AiUsageMachineTotals>,
    /// Per local weekday and hour of day, from Monday midnight on.
    pub hours: Vec<AiUsageHourOfWeekTotals>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageDayTotals {
    pub date: Date,
    pub provider: AiProvider,
    pub model: String,
    pub totals: AiUsageTotals,
}

/// A project's usage, or all unassigned usage when `project` is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageProjectTotals {
    pub project: Option<AiMappedProject>,
    pub totals: AiUsageTotals,
    /// The project keys whose usage counts for it, costliest first.
    pub keys: Vec<AiUsageKeyTotals>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageKeyTotals {
    pub project: AiProjectAttribution,
    pub totals: AiUsageTotals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageModelTotals {
    pub provider: AiProvider,
    pub model: String,
    pub totals: AiUsageTotals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageMachineTotals {
    pub machine_id: AiMachineId,
    pub totals: AiUsageTotals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageHourOfWeekTotals {
    pub weekday: Weekday,
    /// The local hour of day, `0..=23`.
    pub hour: u8,
    pub totals: AiUsageTotals,
}

/// The values a report's filters can take: those with usage in its range.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiUsageFilterOptions {
    pub providers: Vec<AiProvider>,
    /// Model names, costliest first over the whole range.
    pub models: Vec<String>,
    /// Projects by name.
    pub projects: Vec<AiMappedProject>,
    /// Whether any usage in the range is unassigned.
    pub unassigned: bool,
    pub machines: Vec<AiMachineId>,
}

impl AiUsageFilterOptions {
    fn of(sums: AiUsageOptionSums, resolver: &AiMappingResolver) -> Self {
        let mut models = sums.models;
        models.sort_by(|(a_name, a), (b_name, b)| costliest_first(a, b).then(a_name.cmp(b_name)));
        let mut projects: BTreeMap<&str, &AiMappedProject> = BTreeMap::new();
        let mut unassigned = false;
        for key in &sums.project_keys {
            match resolver.project(key) {
                Some(project) => {
                    projects.entry(project.id.as_str()).or_insert(project);
                }
                None => unassigned = true,
            }
        }
        let mut projects = projects.into_values().cloned().collect::<Vec<_>>();
        projects.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });

        Self {
            providers: sums.providers,
            models: models.into_iter().map(|(name, _)| name).collect(),
            projects,
            unassigned,
            machines: sums.machines,
        }
    }
}

impl AiUsageReport {
    /// Attributes the store's sums to projects and orders every breakdown.
    pub fn assemble(
        calendar: AiLocalCalendar,
        dates: AiUsageDateRange,
        resolver: &AiMappingResolver,
        options: AiUsageOptionSums,
        sums: AiUsageSums,
    ) -> Self {
        let mut models = sums.models;
        models.sort_by(|a, b| costliest_first(&a.totals, &b.totals));
        let mut machines = sums.machines;
        machines.sort_by(|a, b| costliest_first(&a.totals, &b.totals));
        let mut days = sums.days;
        days.sort_by(|a, b| (a.date, a.provider, &a.model).cmp(&(b.date, b.provider, &b.model)));
        let mut hours = sums.hours;
        hours.sort_by_key(|hour| (hour.weekday.number_from_monday(), hour.hour));

        Self {
            calendar,
            dates,
            time_tracking_configured: resolver.is_configured(),
            options: AiUsageFilterOptions::of(options, resolver),
            totals: sums.totals,
            days,
            projects: group_by_project(sums.projects, sums.keys, resolver),
            models,
            machines,
            hours,
        }
    }
}

/// Puts each key's sums under the project it counts for, next to the store's
/// exact sum for that project. Groups and keys are costliest first, and
/// Unassigned comes last on a tie.
fn group_by_project(
    projects: Vec<(Option<ProjectId>, AiUsageTotals)>,
    keys: Vec<(String, AiUsageTotals)>,
    resolver: &AiMappingResolver,
) -> Vec<AiUsageProjectTotals> {
    let mut keys_by_project: HashMap<Option<ProjectId>, Vec<AiUsageKeyTotals>> = HashMap::new();
    for (key, totals) in keys {
        let project = AiProjectAttribution::of(key, resolver);
        keys_by_project
            .entry(project.project().map(|project| project.id.clone()))
            .or_default()
            .push(AiUsageKeyTotals { project, totals });
    }

    let mut groups = projects
        .into_iter()
        .map(|(id, totals)| {
            let mut keys = keys_by_project.remove(&id).unwrap_or_default();
            keys.sort_by(|a, b| {
                costliest_first(&a.totals, &b.totals)
                    .then_with(|| a.project.project_key.cmp(&b.project.project_key))
            });
            // Keys mapped to one project were saved separately, so their
            // recorded names can differ; the first key by name decides.
            let project = keys
                .iter()
                .filter_map(|key| key.project.project())
                .min_by(|a, b| a.name.cmp(&b.name))
                .cloned();
            AiUsageProjectTotals {
                project,
                totals,
                keys,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| {
        costliest_first(&a.totals, &b.totals)
            .then(a.project.is_none().cmp(&b.project.is_none()))
            .then_with(|| {
                let name = |group: &AiUsageProjectTotals| {
                    group.project.as_ref().map(|project| project.name.clone())
                };
                name(a).cmp(&name(b))
            })
    });
    groups
}

/// Orders by known cost, then by tokens, largest first. Unknown costs cannot be
/// ranked, so usage is ranked by what is known of it.
fn costliest_first(a: &AiUsageTotals, b: &AiUsageTotals) -> std::cmp::Ordering {
    b.priced_cost_usd
        .total_cmp(&a.priced_cost_usd)
        .then(b.tokens.total().cmp(&a.tokens.total()))
}

/// How a session listing is ordered, largest first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AiSessionOrder {
    /// Most recently active first.
    #[default]
    Recent,
    /// Largest known cost first. A session with unpriced usage ranks by its
    /// known subtotal.
    Cost,
    /// Most tokens first.
    Tokens,
}

impl AiSessionOrder {
    fn as_str(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::Cost => "cost",
            Self::Tokens => "tokens",
        }
    }
}

/// Where a session listing continues: the ordering values of the last session
/// listed, in the order they sort by. Every session sorts after it or before
/// it, never level, so pages neither skip nor repeat sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSessionCursor {
    pub order: AiSessionOrder,
    /// The value the order ranks by, as an exact decimal: the known cost, the
    /// tokens, or the last active hour's Unix time.
    pub rank: String,
    pub last_hour: OffsetDateTime,
    pub first_hour: OffsetDateTime,
    pub machine_id: AiMachineId,
    pub provider: AiProvider,
    pub session_key: String,
}

impl AiSessionCursor {
    const SEPARATOR: char = '~';

    /// The opaque form clients pass back.
    pub fn encode(&self) -> String {
        [
            self.order.as_str(),
            &self.rank,
            &self.last_hour.unix_timestamp().to_string(),
            &self.first_hour.unix_timestamp().to_string(),
            &self.machine_id.to_string(),
            self.provider.as_str(),
            &self.session_key,
        ]
        .join(&Self::SEPARATOR.to_string())
    }

    /// Parses an encoded cursor for a listing in `order`.
    pub fn parse(raw: &str, order: AiSessionOrder) -> Result<Self, AiUsageError> {
        let invalid = || AiUsageError::InvalidQuery("after is not a valid cursor".to_string());
        let parts = raw.split(Self::SEPARATOR).collect::<Vec<_>>();
        let [tag, rank, last_hour, first_hour, machine_id, provider, session_key] =
            parts.as_slice()
        else {
            return Err(invalid());
        };
        if *tag != order.as_str() {
            return Err(AiUsageError::InvalidQuery(
                "after belongs to a listing in another order".to_string(),
            ));
        }
        let decimal = rank.split_once('.').map_or(
            !rank.is_empty() && rank.bytes().all(|byte| byte.is_ascii_digit()),
            |(whole, fraction)| {
                !whole.is_empty()
                    && !fraction.is_empty()
                    && whole
                        .bytes()
                        .chain(fraction.bytes())
                        .all(|b| b.is_ascii_digit())
            },
        );
        let instant = |raw: &str| {
            raw.parse::<i64>()
                .ok()
                .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
        };
        let session_key = session_key.to_string();
        if !decimal
            || session_key.is_empty()
            || !session_key.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid());
        }

        Ok(Self {
            order,
            rank: rank.to_string(),
            last_hour: instant(last_hour).ok_or_else(invalid)?,
            first_hour: instant(first_hour).ok_or_else(invalid)?,
            machine_id: AiMachineId::parse(machine_id).ok_or_else(invalid)?,
            provider: AiProvider::parse(provider).ok_or_else(invalid)?,
            session_key,
        })
    }
}

/// One session as the store sums it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageSessionRow {
    pub machine_id: AiMachineId,
    pub provider: AiProvider,
    pub session_key: String,
    /// By key.
    pub project_keys: Vec<String>,
    /// By name.
    pub models: Vec<String>,
    /// The start of the first and the last UTC hour with usage.
    pub first_hour: OffsetDateTime,
    pub last_hour: OffsetDateTime,
    pub totals: AiUsageTotals,
    /// The value the listing's order ranks the session by, exactly.
    pub rank: String,
}

impl AiUsageSessionRow {
    fn cursor(&self, order: AiSessionOrder) -> AiSessionCursor {
        AiSessionCursor {
            order,
            rank: self.rank.clone(),
            last_hour: self.last_hour,
            first_hour: self.first_hour,
            machine_id: self.machine_id,
            provider: self.provider,
            session_key: self.session_key.clone(),
        }
    }
}

/// Sessions that follow a cursor, as the store lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageSessionRows {
    /// How many sessions the range and filter hold, listed or not.
    pub total: usize,
    pub rows: Vec<AiUsageSessionRow>,
}

/// One session of one tool on one machine, as far as its usage lies in the
/// range and passes the filter.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageSession {
    pub machine_id: AiMachineId,
    pub provider: AiProvider,
    /// Opaque and stable per session.
    pub session_key: String,
    /// The project keys the session's usage was attributed to, by key.
    pub projects: Vec<AiProjectAttribution>,
    /// Model names, by name.
    pub models: Vec<String>,
    pub first_hour: OffsetDateTime,
    pub last_hour: OffsetDateTime,
    pub totals: AiUsageTotals,
}

/// A page of a range's sessions in some order.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageSessionList {
    pub calendar: AiLocalCalendar,
    pub dates: AiUsageDateRange,
    /// How many sessions the range and filter hold, listed or not.
    pub total: usize,
    pub sessions: Vec<AiUsageSession>,
    /// Where the next page starts, or `None` after the last page.
    pub next: Option<AiSessionCursor>,
}

impl AiUsageSessionList {
    /// A page from up to `limit + 1` rows in `order`: a row beyond `limit` only
    /// shows that another page follows.
    pub fn page(
        calendar: AiLocalCalendar,
        dates: AiUsageDateRange,
        order: AiSessionOrder,
        limit: usize,
        rows: AiUsageSessionRows,
        resolver: &AiMappingResolver,
    ) -> Self {
        let AiUsageSessionRows { total, mut rows } = rows;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let next = if more {
            rows.last().map(|row| row.cursor(order))
        } else {
            None
        };

        Self {
            calendar,
            dates,
            total,
            sessions: rows
                .into_iter()
                .map(|row| AiUsageSession {
                    machine_id: row.machine_id,
                    provider: row.provider,
                    session_key: row.session_key,
                    projects: row
                        .project_keys
                        .into_iter()
                        .map(|key| AiProjectAttribution::of(key, resolver))
                        .collect(),
                    models: row.models,
                    first_hour: row.first_hour,
                    last_hour: row.last_hour,
                    totals: row.totals,
                })
                .collect(),
            next,
        }
    }
}

/// Checks how many sessions a page may hold.
pub fn sessions_limit(limit: Option<usize>) -> Result<usize, AiUsageError> {
    let limit = limit.unwrap_or(DEFAULT_SESSIONS_LIMIT);
    if (1..=MAX_SESSIONS_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(AiUsageError::InvalidQuery(format!(
            "limit must be between 1 and {MAX_SESSIONS_LIMIT}"
        )))
    }
}

/// The latest coverage one provider reported from a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMachineCoverage {
    pub coverage: AiProviderCoverage,
    /// The window of the upload that reported it.
    pub window_start: OffsetDateTime,
    pub window_end: OffsetDateTime,
    pub reported_at: OffsetDateTime,
    /// The rates behind the stored costs, or `None` until an upload first
    /// replaced the provider's usage.
    pub pricing: Option<AiPricing>,
    /// Whether the machine has stored usage of the provider. History that is
    /// now missing matters only then.
    pub has_usage: bool,
}

/// A machine registered to the developer and what it last reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMachineStatus {
    pub machine: AiMachine,
    pub first_seen_at: OffsetDateTime,
    pub last_synced_at: OffsetDateTime,
    /// By provider.
    pub coverage: Vec<AiMachineCoverage>,
}

impl AiMachineStatus {
    /// Whether the machine last synced longer than `MACHINE_STALE_AFTER` before `now`.
    pub fn is_stale_at(&self, now: OffsetDateTime) -> bool {
        now - self.last_synced_at > MACHINE_STALE_AFTER
    }
}

/// A machine's status, whether it is stale now, and all its usage in a range.
#[derive(Debug, Clone, PartialEq)]
pub struct AiMachineHealth {
    pub status: AiMachineStatus,
    pub stale: bool,
    /// The machine's usage in the range, whatever a report filters; `None`
    /// without any.
    pub usage: Option<AiUsageTotals>,
}

/// The developer's machines, with their usage in a range.
#[derive(Debug, Clone, PartialEq)]
pub struct AiMachineList {
    pub dates: AiUsageDateRange,
    /// Most recently synced first.
    pub machines: Vec<AiMachineHealth>,
}

/// A project key in the developer's usage, at any time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectKeyUsage {
    pub project: AiProjectAttribution,
    /// The latest local date with usage of the key, in the configured zone.
    pub last_used_on: Date,
}

/// The project keys in the developer's usage, most recently used first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectKeyList {
    /// Whether time tracking is configured. Without it no key resolves and no
    /// key can be mapped.
    pub time_tracking_configured: bool,
    pub keys: Vec<AiProjectKeyUsage>,
}

#[cfg(test)]
mod tests;
