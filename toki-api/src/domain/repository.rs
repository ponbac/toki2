use serde::{Deserialize, Serialize};

use super::RepoKey;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: i32,
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    pub milltime_project_id: Option<String>,
}

impl From<&Repository> for RepoKey {
    fn from(repo: &Repository) -> Self {
        Self::new(&repo.organization, &repo.project, &repo.repo_name)
    }
}
