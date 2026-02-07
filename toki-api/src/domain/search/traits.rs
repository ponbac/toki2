//! Trait definitions for search domain abstractions.
//!
//! These traits enable dependency injection and easy testing through mocking.

use async_trait::async_trait;
use time::OffsetDateTime;

use super::types::{
    ParsedQuery, PullRequestDocument, SearchDocument, SearchResult, SearchSource,
    WorkItemDocument,
};

/// Error type for search operations.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Embedding generation failed: {0}")]
    EmbeddingError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Source fetch error: {0}")]
    SourceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("{0}")]
    Other(String),
}

impl From<sqlx::Error> for SearchError {
    fn from(e: sqlx::Error) -> Self {
        SearchError::DatabaseError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SearchError>;

/// Trait for text embedding generation.
///
/// Abstracts the embedding provider (Gemini, OpenAI, etc.) for easy testing.
///
/// # Example
///
/// ```ignore
/// let embedder = GeminiEmbedder::new(api_key);
/// let embedding = embedder.embed("authentication system").await?;
/// assert_eq!(embedding.len(), 1536); // Gemini embedding dimensions
/// ```
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Generate embedding for a single text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts in a batch.
    ///
    /// Default implementation calls `embed` sequentially.
    /// Implementations should override for better performance.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Returns the embedding dimensions for this embedder.
    #[allow(dead_code)]
    fn dimensions(&self) -> usize;
}

/// Trait for search document persistence and retrieval.
///
/// Abstracts database operations for testing without a real database.
#[async_trait]
pub trait SearchRepository: Send + Sync {
    /// Execute hybrid search (BM25 + vector) with filters.
    ///
    /// When `embedding` is `None`, only BM25 full-text search is used.
    /// Returns results sorted by combined RRF score (or BM25 score if no embedding).
    async fn search(
        &self,
        query: &ParsedQuery,
        embedding: Option<&[f32]>,
        limit: i32,
    ) -> Result<Vec<SearchResult>>;

    /// Insert or update a single document.
    #[allow(dead_code)]
    async fn upsert_document(&self, doc: &SearchDocument) -> Result<()>;

    /// Insert or update multiple documents in a batch.
    ///
    /// Returns the number of documents successfully upserted.
    async fn upsert_documents(&self, docs: &[SearchDocument]) -> Result<usize>;

    /// Delete a document by source type and ID.
    ///
    /// Returns true if a document was deleted.
    #[allow(dead_code)]
    async fn delete_document(&self, source_type: SearchSource, source_id: &str) -> Result<bool>;

    /// Delete all documents that haven't been indexed since the given time.
    ///
    /// Returns the number of documents deleted.
    async fn delete_stale_documents(&self, older_than: OffsetDateTime) -> Result<usize>;

    /// Get a document by source type and ID.
    #[allow(dead_code)]
    async fn get_document(
        &self,
        source_type: SearchSource,
        source_id: &str,
    ) -> Result<Option<SearchDocument>>;

    /// Get total document count, optionally filtered by source type.
    #[allow(dead_code)]
    async fn count(&self, source_type: Option<SearchSource>) -> Result<i64>;
}

/// Trait for fetching documents from Azure DevOps.
///
/// Abstracts ADO API calls for testing without real network requests.
#[async_trait]
pub trait DocumentSource: Send + Sync {
    /// Fetch pull requests from a project.
    ///
    /// Should include active PRs and recently closed ones.
    async fn fetch_pull_requests(
        &self,
        org: &str,
        project: &str,
    ) -> Result<Vec<PullRequestDocument>>;

    /// Fetch work items from a project.
    ///
    /// If `since` is provided, only fetch items updated after that time.
    async fn fetch_work_items(
        &self,
        org: &str,
        project: &str,
        since: Option<OffsetDateTime>,
    ) -> Result<Vec<WorkItemDocument>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify traits are object-safe (can be used as trait objects)
    fn _assert_embedder_object_safe(_: &dyn Embedder) {}
    fn _assert_repository_object_safe(_: &dyn SearchRepository) {}
    fn _assert_source_object_safe(_: &dyn DocumentSource) {}

    #[test]
    fn search_error_from_sqlx() {
        // Just verify the conversion compiles
        let _: SearchError = SearchError::DatabaseError("test".to_string());
    }
}
