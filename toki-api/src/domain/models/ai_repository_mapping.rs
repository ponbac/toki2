//! Team-wide mappings from the repositories a metered payer bills credits to
//! (GitHub Copilot's code review) to time-tracking projects.
//!
//! The payer bills code review twice: per account, under a "Code Review
//! model" label, and per repository, in the organization's rows. The bill
//! takes it once, per repository (`ai_metered_billing`), and these mappings
//! say whose project each repository's credits are.
//!
//! - A mapping points at a time-tracking project directly, so an admin can map
//!   a repository nobody mapped as a project key. It has no validity dates: it
//!   is resolved whenever a bill is read and never written into usage, so a
//!   change applies retroactively, as key mappings do.
//! - Repository names are the payer's bare names, compared case-insensitively
//!   (`repository_key`). Two repositories with the same name under different
//!   owners are one repository to the payer's rows, so they share a mapping.
//! - Only mappings to the configured company resolve (`AiRepositoryResolver`);
//!   a repository without one bills as Unassigned (unmapped repository).
//! - Suggestions come only from project-key mappings that resolve
//!   (`AiMappingResolver::resolved_keys`), never from unmapped keys, which may
//!   name a private repository: the only key data a suggestion reads is the
//!   stored mappings.

use std::collections::{BTreeMap, HashMap};

use time::{Date, OffsetDateTime};

use super::{
    AiBillingMonth, AiMappedProject, AiMappingResolution, AiMappingResolver,
    AiProjectMappingEditor, AiProvider, AiUsdNanos, TimeTrackingCompany,
};
use crate::domain::AiRepositoryMappingError;

/// The longest repository name, as the payer's organization rows store it.
pub const MAX_REPOSITORY_CHARS: usize = 200;

/// What repository names are compared by: their lower case. GitHub's names
/// are ASCII, which Postgres' `lower()` folds the same way.
pub fn repository_key(name: &str) -> String {
    name.to_lowercase()
}

/// A repository name as a request gives it: 1 to 200 characters, kept as
/// given (any spelling the payer billed, padding included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryName(String);

impl AiRepositoryName {
    pub fn parse(raw: &str) -> Result<Self, AiRepositoryMappingError> {
        let invalid = |message: String| Err(AiRepositoryMappingError::InvalidRepository(message));
        let chars = raw.chars().count();
        if chars == 0 {
            return invalid("repository must not be empty".to_string());
        }
        if chars > MAX_REPOSITORY_CHARS {
            return invalid(format!(
                "repository must be at most {MAX_REPOSITORY_CHARS} characters"
            ));
        }
        // Postgres text cannot hold one, so no billed name has one.
        if raw.contains('\0') {
            return invalid("repository must not contain NUL characters".to_string());
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn key(&self) -> String {
        repository_key(&self.0)
    }
}

/// A stored mapping of one of a payer's repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryMapping {
    pub payer: AiProvider,
    /// The repository as the payer spelled it when the mapping was last saved.
    pub repository: String,
    /// The provider company the project belongs to.
    pub company: TimeTrackingCompany,
    pub project: AiMappedProject,
    /// `None` once that user is deleted.
    pub created_by: Option<AiProjectMappingEditor>,
    pub created_at: OffsetDateTime,
    /// `None` once that user is deleted.
    pub updated_by: Option<AiProjectMappingEditor>,
    pub updated_at: OffsetDateTime,
}

/// A listed mapping with how it resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryMappingEntry {
    pub mapping: AiRepositoryMapping,
    pub resolution: AiMappingResolution,
}

/// A stored mapping as the bill needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredRepositoryMapping {
    pub repository: String,
    pub company: TimeTrackingCompany,
    pub project: AiMappedProject,
}

/// Resolves a payer's repositories through its stored mappings against the
/// configured time-tracking company, case-insensitively. The default has no
/// mappings and no configured company: one user's view, which reads no
/// repository rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiRepositoryResolver {
    configured: Option<TimeTrackingCompany>,
    /// By `repository_key`.
    mappings: HashMap<String, AiStoredRepositoryMapping>,
}

impl AiRepositoryResolver {
    /// `configured` is the configured time-tracking company, or `None` when
    /// time tracking is not configured.
    pub fn new(
        mappings: Vec<AiStoredRepositoryMapping>,
        configured: Option<TimeTrackingCompany>,
    ) -> Self {
        Self {
            configured,
            mappings: mappings
                .into_iter()
                .map(|mapping| (repository_key(&mapping.repository), mapping))
                .collect(),
        }
    }

    /// The repository's mapping and how it resolves, or `None` without one.
    pub fn mapping(
        &self,
        repository: &str,
    ) -> Option<(&AiStoredRepositoryMapping, AiMappingResolution)> {
        self.mappings
            .get(&repository_key(repository))
            .map(|mapping| {
                (
                    mapping,
                    AiMappingResolution::of(&mapping.company, self.configured.as_ref()),
                )
            })
    }

    /// The project the repository's credits bill to; `None` is Unassigned
    /// (unmapped repository): no mapping, a stale one or no configured
    /// company.
    pub fn project(&self, repository: &str) -> Option<&AiMappedProject> {
        match self.mapping(repository)? {
            (mapping, AiMappingResolution::Resolves) => Some(&mapping.project),
            _ => None,
        }
    }
}

/// A repository's credits on one UTC day, as stored: one row per spelling
/// and day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryDayCredits {
    pub repository: String,
    pub day: Date,
    pub credits: AiUsdNanos,
}

/// The payer's latest spelling of a repository it billed, by
/// `repository_key`: the spelling of its latest day (on equal days, the
/// greater string, as the bill picks it), or `None` when it never billed it.
pub fn billed_spelling(credits: &[AiRepositoryDayCredits], key: &str) -> Option<String> {
    credits
        .iter()
        .filter(|row| repository_key(&row.repository) == key)
        .max_by(|a, b| (a.day, &a.repository).cmp(&(b.day, &b.repository)))
        .map(|row| row.repository.clone())
}

/// A project a repository could map to: the project of resolving project-key
/// mappings that name the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositorySuggestion {
    pub project: AiMappedProject,
    /// The mapped keys naming the repository, by key; never empty.
    pub project_keys: Vec<String>,
}

/// The projects of resolving project-key mappings `{key_host}/{owner}/{name}`
/// whose name is `repository`: exactly three segments, any non-empty owner,
/// host and name compared case-insensitively. Only `keys.resolved_keys()` is
/// read, so an unmapped key never suggests anything. Grouped by project id,
/// most keys first, then by project name and id.
pub fn suggest_repository_projects(
    repository: &str,
    key_host: &str,
    keys: &AiMappingResolver,
) -> Vec<AiRepositorySuggestion> {
    let wanted = repository_key(repository);
    let host = key_host.to_lowercase();
    let mut by_project: BTreeMap<String, AiRepositorySuggestion> = BTreeMap::new();
    for (key, project) in keys.resolved_keys() {
        let segments: Vec<&str> = key.split('/').collect();
        let [key_host, owner, name] = segments.as_slice() else {
            continue;
        };
        if key_host.to_lowercase() != host
            || owner.is_empty()
            || name.is_empty()
            || repository_key(name) != wanted
        {
            continue;
        }
        by_project
            .entry(project.id.to_string())
            .or_insert_with(|| AiRepositorySuggestion {
                project: project.clone(),
                project_keys: Vec::new(),
            })
            .project_keys
            .push(key.to_string());
    }
    let mut suggestions: Vec<AiRepositorySuggestion> = by_project.into_values().collect();
    suggestions.sort_by(|a, b| {
        b.project_keys
            .len()
            .cmp(&a.project_keys.len())
            .then_with(|| a.project.name.cmp(&b.project.name))
            .then_with(|| a.project.id.as_str().cmp(b.project.id.as_str()))
    });
    suggestions
}

/// A repository's credits in one of the overview's UTC months.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiRepositoryMonthCredits {
    pub month: AiBillingMonth,
    /// Zero without any.
    pub credits: AiUsdNanos,
    /// Whether they bill a line, as `repository_lines` decides: the month is
    /// on or after the metered cutover and its credits, rounded half up to
    /// cents once, come to at least one credit. Credits before the cutover
    /// or under one credit for the month bill nothing, mapped or not.
    pub billed: bool,
}

impl AiRepositoryMonthCredits {
    /// `billed_from` is the metered cutover, `None` without one: then no
    /// month bills.
    pub fn new(
        month: AiBillingMonth,
        credits: AiUsdNanos,
        billed_from: Option<AiBillingMonth>,
    ) -> Self {
        let from_cutover = billed_from.is_some_and(|from| from.first_day() <= month.first_day());
        Self {
            month,
            credits,
            billed: from_cutover && credits.to_cents() > 0,
        }
    }
}

/// One repository of the mapping overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryMappingRow {
    /// The payer's latest spelling, else the mapping's.
    pub repository: String,
    /// The repository's credits in each of the overview's months, in their
    /// order.
    pub months: Vec<AiRepositoryMonthCredits>,
    /// The latest UTC day the payer billed it; `None` for a mapping of a
    /// repository never billed.
    pub last_billed_on: Option<Date>,
    pub mapping: Option<AiRepositoryMappingEntry>,
    /// Empty when the mapping resolves.
    pub suggestions: Vec<AiRepositorySuggestion>,
}

impl AiRepositoryMappingRow {
    /// A line of a shown month bills as Unassigned (unmapped repository):
    /// what needs a look, and what month close will refuse
    /// (`AiMeteredAllocation::unmapped_repositories`). Credits that bill no
    /// line (before the cutover, or under one credit for the month) never
    /// do.
    pub fn needs_mapping(&self) -> bool {
        let resolves = self
            .mapping
            .as_ref()
            .is_some_and(|entry| entry.resolution == AiMappingResolution::Resolves);
        !resolves && self.months.iter().any(|month| month.billed)
    }
}

/// A payer's repositories, their credits and their mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRepositoryMappingOverview {
    pub payer: AiProvider,
    /// The metered cutover: the first month whose bill has repository lines.
    pub billed_from: Option<AiBillingMonth>,
    /// The months credits are shown for, oldest first.
    pub months: Vec<AiBillingMonth>,
    /// Those that need a look (`needs_mapping`) first, then by
    /// `repository_key`.
    pub repositories: Vec<AiRepositoryMappingRow>,
}

/// What the overview is computed from.
pub struct AiRepositoryOverviewInput<'a> {
    pub payer: AiProvider,
    pub months: Vec<AiBillingMonth>,
    pub billed_from: Option<AiBillingMonth>,
    /// Every repository's credits per spelling and UTC day.
    pub credits: Vec<AiRepositoryDayCredits>,
    pub mappings: Vec<AiRepositoryMapping>,
    pub configured: Option<&'a TimeTrackingCompany>,
    /// The stored project-key mappings, the suggestions' only key data.
    pub keys: &'a AiMappingResolver,
    /// The host of the payer's repositories in project keys.
    pub key_host: &'a str,
}

/// The mapping overview: a row for every repository ever billed and every
/// mapping, matched by `repository_key`; credits summed per UTC month, each
/// month marked billed as the bill rounds it; suggestions where no mapping
/// resolves.
pub fn repository_overview(input: AiRepositoryOverviewInput<'_>) -> AiRepositoryMappingOverview {
    #[derive(Default)]
    struct Billed {
        /// The latest (day, spelling).
        latest: Option<(Date, String)>,
        months: Vec<i64>,
    }

    let mut billed: BTreeMap<String, Billed> = BTreeMap::new();
    for row in &input.credits {
        let entry = billed
            .entry(repository_key(&row.repository))
            .or_insert_with(|| Billed {
                latest: None,
                months: vec![0; input.months.len()],
            });
        let candidate = (row.day, row.repository.clone());
        if entry
            .latest
            .as_ref()
            .is_none_or(|latest| candidate > *latest)
        {
            entry.latest = Some(candidate);
        }
        if let Some(index) = input
            .months
            .iter()
            .position(|month| month.contains(row.day))
        {
            entry.months[index] = entry.months[index].saturating_add(row.credits.nanos());
        }
    }
    let mut mappings: BTreeMap<String, AiRepositoryMapping> = input
        .mappings
        .into_iter()
        .map(|mapping| (repository_key(&mapping.repository), mapping))
        .collect();

    let mut keys: Vec<String> = billed.keys().cloned().collect();
    keys.extend(
        mappings
            .keys()
            .filter(|key| !billed.contains_key(*key))
            .cloned(),
    );
    let mut repositories: Vec<(String, AiRepositoryMappingRow)> = keys
        .into_iter()
        .map(|key| {
            let billed = billed.remove(&key);
            let mapping = mappings
                .remove(&key)
                .map(|mapping| AiRepositoryMappingEntry {
                    resolution: AiMappingResolution::of(&mapping.company, input.configured),
                    mapping,
                });
            let latest = billed.as_ref().and_then(|billed| billed.latest.clone());
            let repository = match (&latest, &mapping) {
                (Some((_, spelling)), _) => spelling.clone(),
                (None, Some(entry)) => entry.mapping.repository.clone(),
                (None, None) => key.clone(),
            };
            let sums = billed
                .map(|billed| billed.months)
                .unwrap_or_else(|| vec![0; input.months.len()]);
            let months = input
                .months
                .iter()
                .zip(sums)
                .map(|(month, nanos)| {
                    AiRepositoryMonthCredits::new(
                        *month,
                        AiUsdNanos::new(nanos).unwrap_or_default(),
                        input.billed_from,
                    )
                })
                .collect();
            let resolves = mapping
                .as_ref()
                .is_some_and(|entry| entry.resolution == AiMappingResolution::Resolves);
            let suggestions = if resolves {
                Vec::new()
            } else {
                suggest_repository_projects(&repository, input.key_host, input.keys)
            };
            (
                key,
                AiRepositoryMappingRow {
                    repository,
                    months,
                    last_billed_on: latest.map(|(day, _)| day),
                    mapping,
                    suggestions,
                },
            )
        })
        .collect();
    repositories.sort_by(|(a_key, a), (b_key, b)| {
        (!a.needs_mapping(), a_key).cmp(&(!b.needs_mapping(), b_key))
    });

    AiRepositoryMappingOverview {
        payer: input.payer,
        billed_from: input.billed_from,
        months: input.months,
        repositories: repositories.into_iter().map(|(_, row)| row).collect(),
    }
}

#[cfg(test)]
mod tests;
