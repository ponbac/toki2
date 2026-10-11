//! Team-wide mappings from AI usage project keys to time-tracking projects.
//!
//! token-ledger attributes usage to a project key: a configured project name, a
//! Git remote normalised to `host/owner/repo`, `owner/repo` from Copilot, or
//! `unattributed`. Each key maps to at most one time-tracking project, for
//! everyone's usage. Mappings are resolved when usage is read and never written
//! into usage rows, so a change applies retroactively. Only mappings to the
//! configured provider company resolve; usage without one, including
//! `unattributed` usage, is unassigned.

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
    /// Whether the mapping names a project of the configured company, so usage
    /// of its key resolves to the project. Otherwise the mapping is stale.
    pub fn resolves_for(&self, configured: Option<&TimeTrackingCompany>) -> bool {
        configured == Some(&self.company)
    }
}

/// A listed mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProjectMappingEntry {
    pub mapping: AiProjectMapping,
    /// The mapping names another provider or company than the configured one,
    /// so usage of its key is unassigned until an admin maps it again.
    pub stale: bool,
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
