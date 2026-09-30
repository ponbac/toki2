//! Team-wide mappings from AI usage project keys to time-tracking projects.
//!
//! token-ledger attributes usage to a project key: a configured project name, a
//! Git remote normalised to `host/owner/repo`, `owner/repo` from Copilot, or
//! `unattributed`. Each key maps to at most one time-tracking project, for
//! everyone's usage. Mappings are resolved when usage is read and never written
//! into usage rows, so a change applies retroactively. Only mappings to the
//! configured provider company resolve; usage without one, including
//! `unattributed` usage, is unassigned.

use std::collections::HashMap;

use time::{Date, OffsetDateTime};

use super::{ai_usage::text_problem, ProjectId, TimeTrackingCompany, UserId};
use crate::domain::{AiProjectMappingError, AuthMethod};

/// The key of usage with no project metadata. It cannot be mapped.
pub const UNATTRIBUTED_PROJECT_KEY: &str = "unattributed";

/// A project key that can be mapped: an uploadable key, other than
/// `unattributed`, without surrounding whitespace. Keys are matched exactly, as
/// token-ledger uploads them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiProjectKey(String);

impl AiProjectKey {
    pub fn parse(raw: &str) -> Result<Self, AiProjectMappingError> {
        if let Some(problem) = text_problem("projectKey", raw, true) {
            return Err(AiProjectMappingError::InvalidProjectKey(problem));
        }
        if raw.trim() != raw {
            return Err(AiProjectMappingError::InvalidProjectKey(
                "projectKey must not start or end with whitespace".to_string(),
            ));
        }
        if raw == UNATTRIBUTED_PROJECT_KEY {
            return Err(AiProjectMappingError::Unattributed);
        }

        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The time-tracking project that usage of a project key is attributed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMappedProject {
    pub id: ProjectId,
    /// The project's name when the mapping was last saved, so it stays readable
    /// after the project is archived.
    pub name: String,
}

/// A user who created or last changed a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectMappingEditor {
    pub id: UserId,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectMapping {
    pub project_key: String,
    /// The provider company the project belongs to.
    pub company: TimeTrackingCompany,
    pub project: AiMappedProject,
    /// `None` once that user is deleted.
    pub created_by: Option<AiProjectMappingEditor>,
    pub created_at: OffsetDateTime,
    /// `None` once that user is deleted.
    pub updated_by: Option<AiProjectMappingEditor>,
    pub updated_at: OffsetDateTime,
    /// Whether anyone's stored usage has the key. Admins may map keys before
    /// they are used, and usage can be deleted, so a mapping can be unused.
    pub in_use: bool,
}

impl AiProjectMapping {
    /// How the mapping resolves against the configured company, if any.
    pub fn resolution(&self, configured: Option<&TimeTrackingCompany>) -> AiMappingResolution {
        AiMappingResolution::of(&self.company, configured)
    }
}

/// Whether a stored mapping attributes usage of its key to its project. Every
/// read of usage decides this here, so they cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMappingResolution {
    /// The project belongs to the configured company: usage counts for it.
    Resolves,
    /// The project belongs to another provider or company than the configured
    /// one, so usage is unassigned until an admin maps the key again.
    Stale,
    /// Time tracking is not configured, so no mapping resolves and all usage is
    /// unassigned until it is. Mappings are kept; no admin can fix this in Toki.
    Unconfigured,
}

impl AiMappingResolution {
    /// How a mapping to a project of `company` resolves when `configured` is
    /// the configured time-tracking company, or `None` without one.
    pub fn of(company: &TimeTrackingCompany, configured: Option<&TimeTrackingCompany>) -> Self {
        match configured {
            None => Self::Unconfigured,
            Some(configured) if configured == company => Self::Resolves,
            Some(_) => Self::Stale,
        }
    }
}

/// A listed mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectMappingEntry {
    pub mapping: AiProjectMapping,
    pub resolution: AiMappingResolution,
}

/// A stored mapping as reads of usage need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredMapping {
    pub project_key: String,
    pub company: TimeTrackingCompany,
    pub project: AiMappedProject,
}

/// Resolves project keys through the stored mappings against the configured
/// time-tracking company. Reads of usage attribute keys only through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMappingResolver {
    configured: Option<TimeTrackingCompany>,
    mappings: HashMap<String, AiStoredMapping>,
}

impl AiMappingResolver {
    /// `configured` is the configured time-tracking company, or `None` when
    /// time tracking is not configured.
    pub fn new(mappings: Vec<AiStoredMapping>, configured: Option<TimeTrackingCompany>) -> Self {
        Self {
            configured,
            mappings: mappings
                .into_iter()
                .map(|mapping| (mapping.project_key.clone(), mapping))
                .collect(),
        }
    }

    /// Whether time tracking is configured, so that mappings can resolve and
    /// keys can be mapped.
    pub fn is_configured(&self) -> bool {
        self.configured.is_some()
    }

    /// The key's mapping and how it resolves, or `None` when it is unmapped.
    pub fn mapping(&self, project_key: &str) -> Option<(&AiStoredMapping, AiMappingResolution)> {
        self.mappings.get(project_key).map(|mapping| {
            (
                mapping,
                AiMappingResolution::of(&mapping.company, self.configured.as_ref()),
            )
        })
    }

    /// The project usage of the key counts for, or `None` when it is unassigned.
    pub fn project(&self, project_key: &str) -> Option<&AiMappedProject> {
        match self.mapping(project_key)? {
            (mapping, AiMappingResolution::Resolves) => Some(&mapping.project),
            _ => None,
        }
    }

    /// Every key whose usage counts for a project, with the project, by key.
    pub fn resolved_keys(&self) -> Vec<(&str, &AiMappedProject)> {
        let mut keys = self
            .mappings
            .keys()
            .filter_map(|key| Some((key.as_str(), self.project(key)?)))
            .collect::<Vec<_>>();
        keys.sort_by_key(|(key, _)| *key);
        keys
    }
}

/// A project key that has usage but no mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedAiProjectKey {
    pub project_key: String,
    /// The latest local date, in the configured zone, with usage of the key.
    pub last_used_on: Date,
    /// Whether the key can be mapped. `unattributed` usage and keys with
    /// surrounding whitespace cannot; token-ledger must attribute them first.
    pub mappable: bool,
}

impl UnmappedAiProjectKey {
    pub fn new(project_key: String, last_used_on: Date) -> Self {
        let mappable = AiProjectKey::parse(&project_key).is_ok();

        Self {
            project_key,
            last_used_on,
            mappable,
        }
    }
}

/// Whose usage a query covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiUsageScope {
    User(UserId),
    Everyone,
}

/// Who is managing mappings, as the request authenticated them. The mapping
/// service decides what they may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiProjectMappingActor {
    pub user_id: UserId,
    pub is_admin: bool,
    pub authenticated_by: AuthMethod,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn company(id: &str) -> TimeTrackingCompany {
        TimeTrackingCompany {
            provider: "kleer".to_string(),
            company_id: id.to_string(),
        }
    }

    fn stored(key: &str, company_id: &str, project_id: &str) -> AiStoredMapping {
        AiStoredMapping {
            project_key: key.to_string(),
            company: company(company_id),
            project: AiMappedProject {
                id: ProjectId::new(project_id),
                name: format!("Project {project_id}"),
            },
        }
    }

    #[test]
    fn a_mapping_resolves_only_to_the_configured_company() {
        let configured = company("company-1");
        assert_eq!(
            AiMappingResolution::of(&company("company-1"), Some(&configured)),
            AiMappingResolution::Resolves
        );
        assert_eq!(
            AiMappingResolution::of(&company("company-2"), Some(&configured)),
            AiMappingResolution::Stale
        );
        // Without a configured company nothing resolves, but nothing is stale
        // either: no admin can fix it by mapping again.
        assert_eq!(
            AiMappingResolution::of(&company("company-1"), None),
            AiMappingResolution::Unconfigured
        );
    }

    #[test]
    fn the_resolver_attributes_keys_only_through_resolving_mappings() {
        let mappings = vec![
            stored("b", "company-1", "101"),
            stored("a", "company-1", "202"),
            stored("stale", "company-2", "303"),
        ];
        let resolver = AiMappingResolver::new(mappings.clone(), Some(company("company-1")));
        assert!(resolver.is_configured());
        assert_eq!(resolver.project("b").map(|p| p.id.as_str()), Some("101"));
        assert_eq!(resolver.project("stale"), None);
        assert_eq!(resolver.project("unmapped"), None);
        assert_eq!(
            resolver.mapping("stale").map(|(_, resolution)| resolution),
            Some(AiMappingResolution::Stale)
        );
        assert_eq!(
            resolver
                .resolved_keys()
                .into_iter()
                .map(|(key, project)| (key, project.id.as_str()))
                .collect::<Vec<_>>(),
            [("a", "202"), ("b", "101")]
        );

        let unconfigured = AiMappingResolver::new(mappings, None);
        assert!(!unconfigured.is_configured());
        assert_eq!(unconfigured.project("b"), None);
        assert_eq!(
            unconfigured.mapping("b").map(|(_, resolution)| resolution),
            Some(AiMappingResolution::Unconfigured)
        );
        assert!(unconfigured.resolved_keys().is_empty());
    }

    #[test]
    fn project_keys_are_uploadable_unpadded_keys_other_than_unattributed() {
        for valid in ["github.com/example/app", "example/app", "Client A", "é"] {
            assert_eq!(AiProjectKey::parse(valid).unwrap().as_str(), valid);
        }

        for invalid in [
            String::new(),
            "é".repeat(513),
            "example\0app".to_string(),
            " Client A".to_string(),
            "Client A\t".to_string(),
        ] {
            assert!(
                matches!(
                    AiProjectKey::parse(&invalid),
                    Err(AiProjectMappingError::InvalidProjectKey(_))
                ),
                "{invalid:?}"
            );
        }
        assert!(matches!(
            AiProjectKey::parse(UNATTRIBUTED_PROJECT_KEY),
            Err(AiProjectMappingError::Unattributed)
        ));
    }
}
