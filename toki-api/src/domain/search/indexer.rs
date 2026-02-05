//! Search indexer for syncing data from Azure DevOps to the search index.

use time::{Duration, OffsetDateTime};
use tracing::{info, warn};

use super::traits::{DocumentSource, Embedder, Result, SearchRepository};
use super::types::{
    PullRequestDocument, SearchDocument, SearchSource, SyncStats, WorkItemDocument,
};

/// Configuration for the search indexer.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Batch size for embedding generation
    pub embedding_batch_size: usize,
    /// How old documents can be before being considered stale (hours)
    pub stale_threshold_hours: i64,
    /// Whether to delete stale documents during sync
    pub cleanup_stale: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            embedding_batch_size: 10,
            stale_threshold_hours: 48,
            cleanup_stale: true,
        }
    }
}

/// Indexer for syncing Azure DevOps data to the search index.
///
/// # Type Parameters
///
/// * `E` - Embedder implementation for generating document embeddings
/// * `R` - SearchRepository implementation for database operations
/// * `S` - DocumentSource implementation for fetching from ADO
///
/// # Example
///
/// ```ignore
/// let indexer = SearchIndexer::new(embedder, repository, source, IndexerConfig::default());
/// let stats = indexer.sync_project("org", "project").await?;
/// println!("Indexed {} PRs and {} work items", stats.prs_indexed, stats.work_items_indexed);
/// ```
pub struct SearchIndexer<E, R, S>
where
    E: Embedder,
    R: SearchRepository,
    S: DocumentSource,
{
    embedder: E,
    repository: R,
    source: S,
    config: IndexerConfig,
}

impl<E, R, S> SearchIndexer<E, R, S>
where
    E: Embedder,
    R: SearchRepository,
    S: DocumentSource,
{
    /// Create a new search indexer.
    pub fn new(embedder: E, repository: R, source: S, config: IndexerConfig) -> Self {
        Self {
            embedder,
            repository,
            source,
            config,
        }
    }

    /// Create an indexer with default configuration.
    #[allow(dead_code)]
    pub fn with_defaults(embedder: E, repository: R, source: S) -> Self {
        Self::new(embedder, repository, source, IndexerConfig::default())
    }

    /// Sync all data from a project to the search index.
    pub async fn sync_project(&self, org: &str, project: &str) -> Result<SyncStats> {
        let mut stats = SyncStats::default();
        let sync_start = OffsetDateTime::now_utc();

        info!(org, project, "Starting search index sync");

        // Sync PRs
        match self.sync_pull_requests(org, project).await {
            Ok(count) => {
                stats.prs_indexed = count;
                info!(org, project, count, "Synced pull requests");
            }
            Err(e) => {
                warn!(org, project, error = %e, "Failed to sync pull requests");
                stats.errors += 1;
            }
        }

        // Sync work items
        match self.sync_work_items(org, project, None).await {
            Ok(count) => {
                stats.work_items_indexed = count;
                info!(org, project, count, "Synced work items");
            }
            Err(e) => {
                warn!(org, project, error = %e, "Failed to sync work items");
                stats.errors += 1;
            }
        }

        // Cleanup stale documents
        if self.config.cleanup_stale {
            let stale_threshold =
                sync_start - Duration::hours(self.config.stale_threshold_hours);
            match self.cleanup_stale_documents(stale_threshold).await {
                Ok(count) => {
                    stats.documents_deleted = count;
                    if count > 0 {
                        info!(count, "Cleaned up stale documents");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to cleanup stale documents");
                    stats.errors += 1;
                }
            }
        }

        info!(
            org,
            project,
            prs = stats.prs_indexed,
            work_items = stats.work_items_indexed,
            deleted = stats.documents_deleted,
            errors = stats.errors,
            "Sync completed"
        );

        Ok(stats)
    }

    /// Sync pull requests from ADO to the search index.
    async fn sync_pull_requests(&self, org: &str, project: &str) -> Result<usize> {
        let prs = self.source.fetch_pull_requests(org, project).await?;
        let mut indexed = 0;

        // Process in batches for embedding
        for batch in prs.chunks(self.config.embedding_batch_size) {
            let docs = self.prepare_pr_documents(org, batch).await?;
            indexed += self.repository.upsert_documents(&docs).await?;
        }

        Ok(indexed)
    }

    /// Sync work items from ADO to the search index.
    async fn sync_work_items(
        &self,
        org: &str,
        project: &str,
        since: Option<OffsetDateTime>,
    ) -> Result<usize> {
        let work_items = self.source.fetch_work_items(org, project, since).await?;
        let mut indexed = 0;

        // Process in batches for embedding
        for batch in work_items.chunks(self.config.embedding_batch_size) {
            let docs = self.prepare_work_item_documents(org, batch).await?;
            indexed += self.repository.upsert_documents(&docs).await?;
        }

        Ok(indexed)
    }

    /// Prepare PR documents with embeddings.
    async fn prepare_pr_documents(
        &self,
        org: &str,
        prs: &[PullRequestDocument],
    ) -> Result<Vec<SearchDocument>> {
        let mut documents = Vec::with_capacity(prs.len());

        // Prepare content for batch embedding
        let contents: Vec<String> = prs
            .iter()
            .map(|pr| self.prepare_pr_content(pr))
            .collect();

        let content_refs: Vec<&str> = contents.iter().map(|s| s.as_str()).collect();

        // Generate embeddings in batch
        let embeddings = self.embedder.embed_batch(&content_refs).await?;

        // Build documents
        for (pr, embedding) in prs.iter().zip(embeddings) {
            let source_id = format!("{}/{}/{}/{}", org, pr.project, pr.repo_name, pr.id);

            documents.push(SearchDocument {
                source_type: SearchSource::Pr,
                source_id,
                external_id: pr.id,
                title: pr.title.clone(),
                description: pr.description.clone(),
                content: Some(pr.additional_content.clone()),
                organization: org.to_string(),
                project: pr.project.clone(),
                repo_name: Some(pr.repo_name.clone()),
                status: pr.status.clone(),
                author_id: pr.author_id.clone(),
                author_name: pr.author_name.clone(),
                assigned_to_id: None,
                assigned_to_name: None,
                priority: None,
                item_type: None,
                is_draft: pr.is_draft,
                created_at: pr.created_at,
                updated_at: pr.updated_at,
                closed_at: pr.closed_at,
                url: pr.url.clone(),
                parent_id: None,
                linked_work_items: pr.linked_work_items.clone(),
                embedding: Some(embedding),
            });
        }

        Ok(documents)
    }

    /// Prepare work item documents with embeddings.
    async fn prepare_work_item_documents(
        &self,
        org: &str,
        work_items: &[WorkItemDocument],
    ) -> Result<Vec<SearchDocument>> {
        let mut documents = Vec::with_capacity(work_items.len());

        // Prepare content for batch embedding
        let contents: Vec<String> = work_items
            .iter()
            .map(|wi| self.prepare_work_item_content(wi))
            .collect();

        let content_refs: Vec<&str> = contents.iter().map(|s| s.as_str()).collect();

        // Generate embeddings in batch
        let embeddings = self.embedder.embed_batch(&content_refs).await?;

        // Build documents
        for (wi, embedding) in work_items.iter().zip(embeddings) {
            let source_id = format!("{}/{}/{}", org, wi.project, wi.id);

            documents.push(SearchDocument {
                source_type: SearchSource::WorkItem,
                source_id,
                external_id: wi.id,
                title: wi.title.clone(),
                description: wi.description.clone(),
                content: Some(wi.additional_content.clone()),
                organization: org.to_string(),
                project: wi.project.clone(),
                repo_name: None,
                status: wi.status.clone(),
                author_id: wi.author_id.clone(),
                author_name: wi.author_name.clone(),
                assigned_to_id: wi.assigned_to_id.clone(),
                assigned_to_name: wi.assigned_to_name.clone(),
                priority: wi.priority,
                item_type: Some(wi.item_type.clone()),
                is_draft: false,
                created_at: wi.created_at,
                updated_at: wi.updated_at,
                closed_at: wi.closed_at,
                url: wi.url.clone(),
                parent_id: wi.parent_id,
                linked_work_items: vec![],
                embedding: Some(embedding),
            });
        }

        Ok(documents)
    }

    /// Prepare content for embedding from a PR.
    fn prepare_pr_content(&self, pr: &PullRequestDocument) -> String {
        let mut parts = vec![pr.title.clone()];

        if let Some(ref desc) = pr.description {
            parts.push(desc.clone());
        }

        if !pr.additional_content.is_empty() {
            parts.push(pr.additional_content.clone());
        }

        parts.join("\n\n")
    }

    /// Prepare content for embedding from a work item.
    fn prepare_work_item_content(&self, wi: &WorkItemDocument) -> String {
        let mut parts = vec![wi.title.clone()];

        if let Some(ref desc) = wi.description {
            parts.push(desc.clone());
        }

        if !wi.additional_content.is_empty() {
            parts.push(wi.additional_content.clone());
        }

        parts.join("\n\n")
    }

    /// Remove documents that haven't been updated since the threshold.
    async fn cleanup_stale_documents(&self, older_than: OffsetDateTime) -> Result<usize> {
        self.repository.delete_stale_documents(older_than).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::search::embedder::MockEmbedder;
    use crate::domain::search::repository::MockSearchRepository;
    use async_trait::async_trait;
    use std::sync::{Arc, RwLock};

    // Mock document source for testing
    struct MockDocumentSource {
        prs: Arc<RwLock<Vec<PullRequestDocument>>>,
        work_items: Arc<RwLock<Vec<WorkItemDocument>>>,
    }

    impl MockDocumentSource {
        fn new() -> Self {
            Self {
                prs: Arc::new(RwLock::new(vec![])),
                work_items: Arc::new(RwLock::new(vec![])),
            }
        }

        fn with_prs(self, prs: Vec<PullRequestDocument>) -> Self {
            *self.prs.write().unwrap() = prs;
            self
        }

        fn with_work_items(self, work_items: Vec<WorkItemDocument>) -> Self {
            *self.work_items.write().unwrap() = work_items;
            self
        }
    }

    #[async_trait]
    impl DocumentSource for MockDocumentSource {
        async fn fetch_pull_requests(
            &self,
            _org: &str,
            _project: &str,
        ) -> Result<Vec<PullRequestDocument>> {
            Ok(self.prs.read().unwrap().clone())
        }

        async fn fetch_work_items(
            &self,
            _org: &str,
            _project: &str,
            _since: Option<OffsetDateTime>,
        ) -> Result<Vec<WorkItemDocument>> {
            Ok(self.work_items.read().unwrap().clone())
        }
    }

    fn make_pr(id: i32, title: &str) -> PullRequestDocument {
        PullRequestDocument {
            id,
            title: title.to_string(),
            description: Some("Description".to_string()),
            organization: "org".to_string(),
            project: "project".to_string(),
            repo_name: "repo".to_string(),
            status: "active".to_string(),
            author_id: Some("author".to_string()),
            author_name: Some("Author Name".to_string()),
            is_draft: false,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            closed_at: None,
            url: format!("https://dev.azure.com/org/project/_git/repo/pullrequest/{}", id),
            additional_content: "Commit messages".to_string(),
            linked_work_items: vec![],
        }
    }

    fn make_work_item(id: i32, title: &str, item_type: &str) -> WorkItemDocument {
        WorkItemDocument {
            id,
            title: title.to_string(),
            description: Some("Description".to_string()),
            organization: "org".to_string(),
            project: "project".to_string(),
            status: "Active".to_string(),
            author_id: Some("author".to_string()),
            author_name: Some("Author Name".to_string()),
            assigned_to_id: Some("assignee".to_string()),
            assigned_to_name: Some("Assignee Name".to_string()),
            priority: Some(2),
            item_type: item_type.to_string(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            closed_at: None,
            url: format!("https://dev.azure.com/org/project/_workitems/edit/{}", id),
            parent_id: None,
            additional_content: "Comments".to_string(),
        }
    }

    #[tokio::test]
    async fn sync_indexes_prs_and_work_items() {
        let embedder = MockEmbedder::default();
        let repository = MockSearchRepository::new();
        let source = MockDocumentSource::new()
            .with_prs(vec![make_pr(1, "Auth PR"), make_pr(2, "DB PR")])
            .with_work_items(vec![
                make_work_item(100, "Auth Bug", "Bug"),
                make_work_item(101, "DB Task", "Task"),
            ]);

        let config = IndexerConfig {
            cleanup_stale: false,
            ..Default::default()
        };

        let indexer = SearchIndexer::new(embedder, repository.clone(), source, config);
        let stats = indexer.sync_project("org", "project").await.unwrap();

        assert_eq!(stats.prs_indexed, 2);
        assert_eq!(stats.work_items_indexed, 2);
        assert_eq!(stats.errors, 0);
        assert_eq!(repository.len(), 4);
    }

    #[tokio::test]
    async fn sync_generates_embeddings() {
        let embedder = MockEmbedder::default();
        let repository = MockSearchRepository::new();
        let source = MockDocumentSource::new()
            .with_prs(vec![make_pr(1, "Test PR")]);

        let indexer = SearchIndexer::with_defaults(embedder.clone(), repository, source);
        indexer.sync_project("org", "project").await.unwrap();

        // Should have called embed_batch once with 1 text
        assert_eq!(embedder.call_count(), 1);
    }

    #[tokio::test]
    async fn sync_handles_empty_source() {
        let embedder = MockEmbedder::default();
        let repository = MockSearchRepository::new();
        let source = MockDocumentSource::new();

        let indexer = SearchIndexer::with_defaults(embedder, repository.clone(), source);
        let stats = indexer.sync_project("org", "project").await.unwrap();

        assert_eq!(stats.prs_indexed, 0);
        assert_eq!(stats.work_items_indexed, 0);
        assert_eq!(stats.total_indexed(), 0);
        assert!(repository.is_empty());
    }
}
