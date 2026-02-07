//! Azure DevOps document source implementation.
//!
//! Wraps the az-devops crate to fetch PRs and work items for search indexing.

use async_trait::async_trait;
use futures::{stream, StreamExt, TryStreamExt};
use time::OffsetDateTime;

use az_devops::RepoClient;

use crate::domain::search::traits::{DocumentSource, Result, SearchError};
use crate::domain::search::types::{PullRequestDocument, WorkItemDocument};

/// Document source that fetches from Azure DevOps.
///
/// # Example
///
/// ```ignore
/// let client = RepoClient::new("repo", "org", "project", "pat").await?;
/// let source = AdoDocumentSource::new(client, "repo".to_string());
/// let prs = source.fetch_pull_requests("org", "project").await?;
/// ```
pub struct AdoDocumentSource {
    client: RepoClient,
    repo_name: String,
}

impl AdoDocumentSource {
    /// Create a new ADO document source.
    pub fn new(client: RepoClient, repo_name: String) -> Self {
        Self { client, repo_name }
    }
}

#[async_trait]
impl DocumentSource for AdoDocumentSource {
    async fn fetch_pull_requests(
        &self,
        org: &str,
        project: &str,
    ) -> Result<Vec<PullRequestDocument>> {
        // Fetch all PRs (open and closed)
        let prs = self
            .client
            .get_all_pull_requests(None)
            .await
            .map_err(|e| SearchError::SourceError(e.to_string()))?;

        let org = org.to_string();
        let project = project.to_string();
        let repo_name = self.repo_name.clone();

        // Process PRs in parallel, fetching threads, commits, and work items concurrently
        let documents: Vec<PullRequestDocument> = stream::iter(prs)
            .map(|pr| {
                let client = &self.client;
                let org = org.clone();
                let project = project.clone();
                let repo_name = repo_name.clone();

                async move {
                    // Fetch threads, commits, and work items in parallel for this PR
                    let (threads, commits, work_item_ids) = tokio::try_join!(
                        client.get_threads_in_pull_request(pr.id),
                        client.get_commits_in_pull_request(pr.id),
                        client.get_work_item_ids_in_pull_request(pr.id),
                    )
                    .map_err(|e| SearchError::SourceError(e.to_string()))?;

                    // Combine comments from all threads
                    let mut comment_texts = Vec::new();
                    for thread in threads {
                        for comment in thread.comments {
                            if let Some(content) = comment.content {
                                comment_texts.push(content);
                            }
                        }
                    }

                    // Combine commit messages
                    let mut commit_messages = Vec::new();
                    for commit in commits {
                        if let Some(comment) = commit.comment {
                            commit_messages.push(comment);
                        }
                    }

                    // Build additional content from comments and commits
                    let additional_content = format!(
                        "{}\n\n{}",
                        comment_texts.join("\n\n"),
                        commit_messages.join("\n")
                    );

                    // Map PR status to string
                    let status = format!("{:?}", pr.status);

                    Ok::<_, SearchError>(PullRequestDocument {
                        id: pr.id,
                        title: pr.title,
                        description: pr.description,
                        organization: org,
                        project,
                        repo_name,
                        status,
                        author_id: Some(pr.created_by.id.clone()),
                        author_name: Some(pr.created_by.display_name.clone()),
                        is_draft: pr.is_draft,
                        created_at: pr.created_at,
                        updated_at: pr.created_at, // PRs don't have updated_at in the model
                        closed_at: pr.closed_at,
                        url: pr.url,
                        additional_content,
                        linked_work_items: work_item_ids,
                    })
                }
            })
            .buffer_unordered(10)
            .try_collect()
            .await?;

        Ok(documents)
    }

    async fn fetch_work_items(
        &self,
        org: &str,
        project: &str,
        _since: Option<OffsetDateTime>,
    ) -> Result<Vec<WorkItemDocument>> {
        // Fetch work item IDs from PRs
        let prs = self
            .client
            .get_all_pull_requests(None)
            .await
            .map_err(|e| SearchError::SourceError(e.to_string()))?;

        // Fetch work item IDs in parallel
        let all_work_item_ids: Vec<Vec<i32>> = stream::iter(prs)
            .map(|pr| {
                let client = &self.client;
                async move {
                    client
                        .get_work_item_ids_in_pull_request(pr.id)
                        .await
                        .map_err(|e| SearchError::SourceError(e.to_string()))
                }
            })
            .buffer_unordered(10)
            .try_collect()
            .await?;

        // Flatten and remove duplicates
        let mut all_work_item_ids: Vec<i32> = all_work_item_ids.into_iter().flatten().collect();
        all_work_item_ids.sort_unstable();
        all_work_item_ids.dedup();

        if all_work_item_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch work item details
        let work_items = self
            .client
            .get_work_items(all_work_item_ids)
            .await
            .map_err(|e| SearchError::SourceError(e.to_string()))?;

        let documents = work_items
            .into_iter()
            .map(|wi| {
                // For work items, we don't have an easy way to get comments
                // This would require additional API calls to get work item comments
                // For now, leaving additional_content empty
                let additional_content = String::new();

                WorkItemDocument {
                    id: wi.id,
                    title: wi.title,
                    description: None, // Work items don't expose description in the current model
                    organization: org.to_string(),
                    project: project.to_string(),
                    status: wi.state,
                    author_id: wi.created_by.as_ref().map(|i| i.id.clone()),
                    author_name: wi.created_by.as_ref().map(|i| i.display_name.clone()),
                    assigned_to_id: wi.assigned_to.as_ref().map(|i| i.id.clone()),
                    assigned_to_name: wi.assigned_to.as_ref().map(|i| i.display_name.clone()),
                    priority: wi.priority,
                    item_type: wi.item_type,
                    created_at: wi.created_at,
                    updated_at: wi.changed_at,
                    closed_at: None, // Work items don't have closed_at in the current model
                    url: format!(
                        "https://dev.azure.com/{}/{}/_workitems/edit/{}",
                        org, project, wi.id
                    ),
                    parent_id: wi.parent_id,
                    additional_content,
                }
            })
            .collect();

        Ok(documents)
    }
}

#[cfg(test)]
mod tests {
    // Note: Real tests would require mocking the RepoClient
    // or using an actual ADO connection (which requires credentials)
}
