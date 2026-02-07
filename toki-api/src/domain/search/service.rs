//! Search service combining embedding generation and hybrid search.

use super::parser::parse_query;
use super::traits::{Embedder, Result, SearchRepository};
use super::types::SearchResult;

/// Configuration for the search service.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Default number of results to return
    pub default_limit: i32,
    /// Maximum number of results allowed
    pub max_limit: i32,
    /// Minimum query length for semantic search
    pub min_query_length: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_limit: 20,
            max_limit: 100,
            min_query_length: 2,
        }
    }
}

/// Search service that combines embedding generation with hybrid search.
///
/// # Type Parameters
///
/// * `E` - Embedder implementation for generating query embeddings
/// * `R` - SearchRepository implementation for database operations
///
/// # Examples
///
/// ```ignore
/// let service = SearchService::new(embedder, repository, SearchConfig::default());
/// let results = service.search("authentication PRs", 10).await?;
/// ```
pub struct SearchService<E, R>
where
    E: Embedder,
    R: SearchRepository,
{
    embedder: E,
    repository: R,
    config: SearchConfig,
}

impl<E, R> SearchService<E, R>
where
    E: Embedder,
    R: SearchRepository,
{
    /// Create a new search service.
    pub fn new(embedder: E, repository: R, config: SearchConfig) -> Self {
        Self {
            embedder,
            repository,
            config,
        }
    }

    /// Create a search service with default configuration.
    pub fn with_defaults(embedder: E, repository: R) -> Self {
        Self::new(embedder, repository, SearchConfig::default())
    }

    /// Execute a search query.
    ///
    /// Parses the query to extract filters, generates an embedding for semantic search,
    /// and performs hybrid BM25 + vector search with RRF fusion.
    ///
    /// # Arguments
    ///
    /// * `query` - Natural language search query
    /// * `limit` - Maximum number of results (None uses default, capped at max_limit)
    ///
    /// # Returns
    ///
    /// Results sorted by combined relevance score (higher is better).
    pub async fn search(&self, query: &str, limit: Option<i32>) -> Result<Vec<SearchResult>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(vec![]);
        }

        // Parse query into filters and search text
        let parsed = parse_query(query);

        // Determine effective limit
        let limit = limit
            .unwrap_or(self.config.default_limit)
            .min(self.config.max_limit)
            .max(1);

        // Generate embedding for semantic search (skip for filter-only queries)
        let embedding = if parsed.search_text.len() >= self.config.min_query_length {
            Some(self.embedder.embed(&parsed.search_text).await?)
        } else {
            None
        };

        // Execute hybrid search (or BM25-only when no embedding)
        self.repository
            .search(&parsed, embedding.as_deref(), limit)
            .await
    }

    /// Get document counts by source type.
    #[allow(dead_code)]
    pub async fn stats(&self) -> Result<SearchStats> {
        let total = self.repository.count(None).await?;
        let prs = self
            .repository
            .count(Some(super::types::SearchSource::Pr))
            .await?;
        let work_items = self
            .repository
            .count(Some(super::types::SearchSource::WorkItem))
            .await?;

        Ok(SearchStats {
            total,
            prs,
            work_items,
        })
    }
}

/// Statistics about the search index.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SearchStats {
    pub total: i64,
    pub prs: i64,
    pub work_items: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::search::embedder::MockEmbedder;
    use crate::domain::search::repository::MockSearchRepository;
    use crate::domain::search::types::{SearchDocument, SearchSource};
    use time::OffsetDateTime;

    fn make_pr(id: &str, title: &str) -> SearchDocument {
        SearchDocument {
            source_type: SearchSource::Pr,
            source_id: id.to_string(),
            external_id: 1,
            title: title.to_string(),
            description: Some("Description".to_string()),
            content: None,
            organization: "org".to_string(),
            project: "project".to_string(),
            repo_name: Some("repo".to_string()),
            status: "active".to_string(),
            author_id: None,
            author_name: Some("Author".to_string()),
            assigned_to_id: None,
            assigned_to_name: None,
            priority: None,
            item_type: None,
            is_draft: false,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            closed_at: None,
            url: "https://dev.azure.com/org/project/_git/repo/pullrequest/1".to_string(),
            parent_id: None,
            linked_work_items: vec![],
            embedding: None,
        }
    }

    fn make_bug(id: &str, title: &str, priority: i32) -> SearchDocument {
        SearchDocument {
            source_type: SearchSource::WorkItem,
            source_id: id.to_string(),
            external_id: 1,
            title: title.to_string(),
            description: Some("Bug description".to_string()),
            content: None,
            organization: "org".to_string(),
            project: "Lerums Djursjukhus".to_string(),
            repo_name: None,
            status: "active".to_string(),
            author_id: None,
            author_name: Some("Reporter".to_string()),
            assigned_to_id: None,
            assigned_to_name: None,
            priority: Some(priority),
            item_type: Some("Bug".to_string()),
            is_draft: false,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            closed_at: None,
            url: "https://dev.azure.com/org/project/_workitems/edit/1".to_string(),
            parent_id: None,
            linked_work_items: vec![],
            embedding: None,
        }
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new();
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("", None).await.unwrap();
        assert!(results.is_empty());

        let results = service.search("   ", None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_returns_matching_documents() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new().with_documents(vec![
            make_pr("pr/1", "Authentication fix"),
            make_pr("pr/2", "Database migration"),
        ]);
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("authentication", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("Authentication"));
    }

    #[tokio::test]
    async fn search_applies_source_type_filter() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new().with_documents(vec![
            make_pr("pr/1", "Auth PR"),
            make_bug("wi/1", "Auth Bug", 1),
        ]);
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("auth PRs", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_type, SearchSource::Pr);
    }

    #[tokio::test]
    async fn search_applies_priority_filter() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new().with_documents(vec![
            make_bug("wi/1", "Critical bug", 1),
            make_bug("wi/2", "Minor bug", 3),
        ]);
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("priority 1 bugs", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].priority, Some(1));
    }

    #[tokio::test]
    async fn search_applies_project_filter() {
        let embedder = MockEmbedder::default();
        let lerum_bug = make_bug("wi/1", "Lerum bug", 1);
        let mut other_bug = make_bug("wi/2", "Other bug", 1);
        other_bug.project = "Other Project".to_string();

        let repo = MockSearchRepository::new().with_documents(vec![lerum_bug, other_bug]);
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("bugs in Lerum", None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].source_id.contains("wi/1"));
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new().with_documents(vec![
            make_pr("pr/1", "PR 1"),
            make_pr("pr/2", "PR 2"),
            make_pr("pr/3", "PR 3"),
        ]);
        let service = SearchService::with_defaults(embedder, repo);

        let results = service.search("PR", Some(2)).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_generates_embedding_for_semantic_search() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new();
        let service = SearchService::with_defaults(embedder.clone(), repo);

        service.search("authentication", None).await.unwrap();

        // Embedder should have been called once
        assert_eq!(embedder.call_count(), 1);
    }

    #[tokio::test]
    async fn search_skips_embedding_for_short_queries() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new();
        let service = SearchService::with_defaults(embedder.clone(), repo);

        // Single character query
        service.search("a", None).await.unwrap();

        // Embedder should NOT have been called
        assert_eq!(embedder.call_count(), 0);
    }

    #[tokio::test]
    async fn stats_returns_counts() {
        let embedder = MockEmbedder::default();
        let repo = MockSearchRepository::new().with_documents(vec![
            make_pr("pr/1", "PR 1"),
            make_pr("pr/2", "PR 2"),
            make_bug("wi/1", "Bug 1", 1),
        ]);
        let service = SearchService::with_defaults(embedder, repo);

        let stats = service.stats().await.unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.prs, 2);
        assert_eq!(stats.work_items, 1);
    }
}
