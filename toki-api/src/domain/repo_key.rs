use std::fmt::{self, Display};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RepoKey {
    pub organization: String,
    pub project: String,
    pub repo_name: String,
}

impl Display for RepoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            self.organization, self.project, self.repo_name
        )
    }
}

impl RepoKey {
    pub fn new(organization: &str, project: &str, repo_name: &str) -> Self {
        Self {
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_name: repo_name.to_owned(),
        }
    }
}
