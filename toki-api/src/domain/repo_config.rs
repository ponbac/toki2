use az_devops::RepoClient;
use serde::Deserialize;

use super::RepoKey;

#[derive(Deserialize)]
pub struct RepoConfig {
    pub id: i32,
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    pub token: String,
}

impl RepoConfig {
    pub async fn to_client(&self) -> Result<RepoClient, az_devops::RepoClientError> {
        let repo_client = RepoClient::new(
            &self.repo_name,
            &self.organization,
            &self.project,
            &self.token,
        )
        .await?;

        Ok(repo_client)
    }

    pub fn key(&self) -> RepoKey {
        RepoKey {
            organization: self.organization.clone(),
            project: self.project.clone(),
            repo_name: self.repo_name.clone(),
        }
    }
}

impl From<&RepoConfig> for RepoKey {
    fn from(repo: &RepoConfig) -> Self {
        repo.key()
    }
}
