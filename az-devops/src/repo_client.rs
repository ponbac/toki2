use azure_devops_rust_api::{
    git::{self, models::GitCommitRef},
    wit::{
        self,
        models::{work_item_batch_get_request::Expand, WorkItemBatchGetRequest},
    },
    Credential,
};

use crate::{models::PullRequest, Thread, WorkItem};

pub struct RepoClient {
    git_client: git::Client,
    work_item_client: wit::Client,
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
        let git_client = git::ClientBuilder::new(credential.clone()).build();
        let work_item_client = wit::ClientBuilder::new(credential).build();

        let repo = git_client
            .repositories_client()
            .list(organization, project)
            .await?
            .value
            .iter()
            .find(|repo| repo.name == repo_name)
            .cloned()
            .ok_or_else(|| format!("Repo {} not found", repo_name))?;

        Ok(Self {
            git_client,
            work_item_client,
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_id: repo.id,
        })
    }

    pub async fn get_open_pull_requests(
        &self,
    ) -> Result<Vec<PullRequest>, Box<dyn std::error::Error>> {
        let pull_requests = self
            .git_client
            .pull_requests_client()
            .get_pull_requests(&self.organization, &self.repo_id, &self.project)
            .await?
            .value;

        Ok(pull_requests.into_iter().map(PullRequest::from).collect())
    }

    pub async fn get_all_pull_requests(
        &self,
    ) -> Result<Vec<PullRequest>, Box<dyn std::error::Error>> {
        let mut pull_requests = vec![];
        let mut skip = 0;
        let top = 100;

        loop {
            let mut page = self
                .git_client
                .pull_requests_client()
                .get_pull_requests(&self.organization, &self.repo_id, &self.project)
                .search_criteria_status("all")
                .skip(skip)
                .top(top)
                .await?
                .value;

            if page.is_empty() {
                break;
            }

            pull_requests.append(&mut page);

            skip += top;
        }

        Ok(pull_requests.into_iter().map(PullRequest::from).collect())
    }

    pub async fn get_threads_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<Thread>, Box<dyn std::error::Error>> {
        let threads = self
            .git_client
            .pull_request_threads_client()
            .list(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(threads
            .into_iter()
            .map(|t| Thread::from(t.comment_thread))
            .collect())
    }

    pub async fn get_work_item_ids_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
        let work_item_refs = self
            .git_client
            .pull_request_work_items_client()
            .list(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(work_item_refs
            .into_iter()
            .filter_map(|r| r.id)
            .map(|id| id.parse().unwrap())
            .collect())
    }

    pub async fn get_commits_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<GitCommitRef>, Box<dyn std::error::Error>> {
        let commits = self
            .git_client
            .pull_request_commits_client()
            .get_pull_request_commits(
                &self.organization,
                &self.repo_id,
                pull_request_id,
                &self.project,
            )
            .await?
            .value;

        Ok(commits)
    }

    pub async fn get_work_items(
        &self,
        ids: Vec<i32>,
    ) -> Result<Vec<WorkItem>, Box<dyn std::error::Error>> {
        let mut batch_request = WorkItemBatchGetRequest::new();
        batch_request.expand = Some(Expand::Relations);
        batch_request.ids = ids;

        let work_items = self
            .work_item_client
            .work_items_client()
            .get_work_items_batch(&self.organization, batch_request, &self.project)
            .await?
            .value;

        Ok(work_items.into_iter().map(WorkItem::from).collect())
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

    #[tokio::test]
    async fn test_get_pull_request_threads() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        assert!(!pull_requests.is_empty());

        let test_pr = &pull_requests[0];
        let threads = repo_client
            .get_threads_in_pull_request(test_pr.id)
            .await
            .unwrap();

        assert!(!threads.is_empty());
    }

    #[tokio::test]
    async fn test_get_pull_request_commits() {
        let repo_client = get_repo_client().await;
        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

        assert!(!pull_requests.is_empty());

        let test_pr = &pull_requests[0];
        let commits = repo_client
            .get_commits_in_pull_request(test_pr.id)
            .await
            .unwrap();

        assert!(!commits.is_empty());
    }

    #[tokio::test]
    async fn test_get_work_items() {
        let repo_client = get_repo_client().await;

        let pull_requests = repo_client.get_open_pull_requests().await.unwrap();
        assert!(!pull_requests.is_empty());

        let test_pr = pull_requests
            .iter()
            .find(|pr| pr.title == "[FE] Edit subgroups")
            .unwrap();

        let work_item_ids = repo_client
            .get_work_item_ids_in_pull_request(test_pr.id)
            .await
            .unwrap();
        assert!(!work_item_ids.is_empty());

        let work_items = repo_client.get_work_items(work_item_ids).await.unwrap();
        assert!(!work_items.is_empty());
    }
}
