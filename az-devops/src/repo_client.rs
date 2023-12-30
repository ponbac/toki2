use azure_devops_rust_api::{
    git::{self},
    Credential,
};

use crate::models::PullRequest;

pub struct RepoClient {
    client: git::Client,
    organization: String,
    project: String,
    repo_id: String,
}

impl RepoClient {
    pub async fn new(
        repo_name: &str,
        organization: &str,
        project: &str,
        pat: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let credential = Credential::from_pat(pat.to_owned());
        let client = git::ClientBuilder::new(credential).build();

        let repo = client
            .repositories_client()
            .list(organization, project)
            .await?
            .value
            .iter()
            .find(|repo| repo.name == repo_name)
            .cloned()
            .ok_or_else(|| format!("Repo {} not found", repo_name))?;

        Ok(Self {
            client,
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_id: repo.id,
        })
    }

    pub async fn get_open_pull_requests(
        &self,
    ) -> Result<Vec<PullRequest>, Box<dyn std::error::Error>> {
        let pull_requests = self
            .client
            .pull_requests_client()
            .get_pull_requests(&self.organization, &self.repo_id, &self.project)
            .await?
            .value;

        Ok(pull_requests
            .into_iter()
            .map(PullRequest::from)
            .collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get_repo_client() -> RepoClient {
        dotenvy::from_filename(".env.local").ok();

        RepoClient::new(
            &std::env::var("ADO_REPO").unwrap(),
            &std::env::var("ADO_ORGANIZATION").unwrap(),
            &std::env::var("ADO_PROJECT").unwrap(),
            &std::env::var("ADO_TOKEN").unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_get_open_pull_requests() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        assert!(!pull_requests.is_empty());
    }
}
