use std::collections::HashSet;

use azure_devops_rust_api::{
    git::{self, models::GitCommitRef},
    graph::{self, models::GraphUser},
    wit::{
        self,
        models::{work_item_batch_get_request::Expand, WorkItemBatchGetRequest},
    },
    Credential,
};
use futures::StreamExt;

use crate::{models::PullRequest, Identity, Thread, WorkItem};

#[derive(Debug, thiserror::Error)]
pub enum RepoClientError {
    #[error("Azure DevOps API error: {0}")]
    AzureDevOpsError(#[from] typespec::error::Error),
    #[error("Repository not found: {0}")]
    RepoNotFound(String),
}

#[derive(Clone)]
pub struct RepoClient {
    git_client: git::Client,
    work_item_client: wit::Client,
    graph_client: graph::Client,
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
    ) -> Result<Self, RepoClientError> {
        // might need to disable retries or set a timeout (https://docs.rs/azure_devops_rust_api/latest/azure_devops_rust_api/git/struct.ClientBuilder.html, https://docs.rs/azure_core/0.20.0/azure_core/struct.TimeoutPolicy.html)
        let credential = Credential::from_pat(pat.to_owned());
        let git_client = git::ClientBuilder::new(credential.clone()).build();
        let work_item_client = wit::ClientBuilder::new(credential.clone()).build();
        let graph_client = graph::ClientBuilder::new(credential).build();

        let repo = git_client
            .repositories_client()
            .list(organization, project)
            .await?
            .value
            .iter()
            .find(|repo| repo.name == repo_name)
            .cloned()
            .ok_or_else(|| RepoClientError::RepoNotFound(repo_name.to_string()))?;

        Ok(Self {
            git_client,
            work_item_client,
            graph_client,
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_id: repo.id,
        })
    }

    pub async fn get_open_pull_requests(&self) -> Result<Vec<PullRequest>, RepoClientError> {
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
        limit: Option<usize>,
    ) -> Result<Vec<PullRequest>, RepoClientError> {
        const PAGE_SIZE: i32 = 50;
        let max_items = limit.unwrap_or(usize::MAX);

        if max_items == 0 {
            return Ok(Vec::new());
        }

        let mut pull_requests = Vec::new();
        let mut skip = 0;

        loop {
            let page = self
                .git_client
                .pull_requests_client()
                .get_pull_requests(&self.organization, &self.repo_id, &self.project)
                .search_criteria_status("all")
                .skip(skip)
                .top(PAGE_SIZE)
                .await?
                .value;

            if page.is_empty() {
                break;
            }

            let remaining_capacity = max_items.saturating_sub(pull_requests.len());
            pull_requests.extend(
                page.into_iter()
                    .take(remaining_capacity)
                    .map(PullRequest::from),
            );

            if pull_requests.len() >= max_items {
                break;
            }

            skip += PAGE_SIZE;
        }

        Ok(pull_requests)
    }

    pub async fn get_threads_in_pull_request(
        &self,
        pull_request_id: i32,
    ) -> Result<Vec<Thread>, RepoClientError> {
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
    ) -> Result<Vec<i32>, RepoClientError> {
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
    ) -> Result<Vec<GitCommitRef>, RepoClientError> {
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

    pub async fn get_work_items(&self, ids: Vec<i32>) -> Result<Vec<WorkItem>, RepoClientError> {
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

    // TODO: how to handle continuation token?
    pub async fn get_graph_users(&self) -> Result<Vec<GraphUser>, RepoClientError> {
        let user_list_response = self
            .graph_client
            .users_client()
            .list(&self.organization)
            .await?;

        if user_list_response.count.is_none_or(|count| count == 0) {
            return Ok(vec![]);
        }

        Ok(user_list_response.value)
    }

    /// Workaround to get all identities as there is no way to list all identities with
    /// the same ID that is used in the git API.
    pub async fn get_git_identities(&self) -> Result<Vec<Identity>, RepoClientError> {
        const MAX_PULL_REQUESTS: usize = 100;
        const CONCURRENCY: usize = 10;

        let pull_requests = self.get_all_pull_requests(Some(MAX_PULL_REQUESTS)).await?;
        let threads = futures::stream::iter(pull_requests.iter())
            .map(|pr| pr.threads(self))
            .buffer_unordered(CONCURRENCY)
            .filter_map(|result| async { result.ok() })
            .flat_map(futures::stream::iter)
            .collect::<Vec<_>>()
            .await;

        let mut identities = HashSet::new();
        for pull_request in pull_requests {
            identities.insert(pull_request.created_by);
            pull_request.reviewers.iter().for_each(|reviewer| {
                identities.insert(reviewer.identity.clone());
            });
        }

        for thread in threads {
            thread.comments.iter().for_each(|comment| {
                identities.insert(comment.author.clone());
            });
        }

        Ok(identities.into_iter().collect())
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
            .find(|pr| pr.title == "Make export of sell prices more robust")
            .unwrap();

        let work_item_ids = repo_client
            .get_work_item_ids_in_pull_request(test_pr.id)
            .await
            .unwrap();
        assert!(!work_item_ids.is_empty());

        let work_items = repo_client.get_work_items(work_item_ids).await.unwrap();
        assert!(!work_items.is_empty());
    }

    #[tokio::test]
    async fn test_get_graph_users() {
        let repo_client = get_repo_client().await;
        let identities = repo_client.get_graph_users().await.unwrap();
        assert!(!identities.is_empty());
    }

    #[tokio::test]
    async fn test_get_git_identities() {
        let repo_client = get_repo_client().await;
        let identities = repo_client.get_git_identities().await.unwrap();

        for identity in &identities {
            println!(
                "Name: {}, Email: {}, ID: {}",
                identity.display_name, identity.unique_name, identity.id
            );
        }

        assert!(!identities.is_empty());
    }
}
