use super::{ActivityId, ProjectId};

/// A project in the time tracking system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    /// Optional project number/code (e.g., "ABC-123").
    pub code: Option<String>,
}

impl Project {
    pub fn new(id: impl Into<ProjectId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            code: None,
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

/// An activity within a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: ActivityId,
    pub name: String,
    pub project_id: ProjectId,
}

impl Activity {
    pub fn new(
        id: impl Into<ActivityId>,
        name: impl Into<String>,
        project_id: impl Into<ProjectId>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            project_id: project_id.into(),
        }
    }
}
