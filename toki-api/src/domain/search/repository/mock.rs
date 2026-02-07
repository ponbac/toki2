//! Mock repository implementation for testing.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;

use crate::domain::search::traits::{Result, SearchRepository};
use crate::domain::search::types::{ParsedQuery, SearchDocument, SearchResult, SearchSource};

/// Mock search repository backed by an in-memory HashMap.
///
/// # Examples
///
/// ```
/// use toki_api::domain::search::repository::MockSearchRepository;
///
/// let repo = MockSearchRepository::new();
/// // or with initial documents:
/// let repo = MockSearchRepository::new().with_documents(vec![doc1, doc2]);
/// ```
#[derive(Clone, Default)]
pub struct MockSearchRepository {
    documents: Arc<RwLock<HashMap<(SearchSource, String), SearchDocument>>>,
    /// Custom search results to return (overrides default behavior)
    custom_results: Arc<RwLock<Option<Vec<SearchResult>>>>,
}

#[allow(dead_code)]
impl MockSearchRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add initial documents to the repository.
    pub fn with_documents(self, docs: Vec<SearchDocument>) -> Self {
        {
            let mut documents = self.documents.write().unwrap();
            for doc in docs {
                let key = (doc.source_type, doc.source_id.clone());
                documents.insert(key, doc);
            }
        }
        self
    }

    /// Configure custom search results to return.
    pub fn with_search_results(self, results: Vec<SearchResult>) -> Self {
        {
            let mut custom = self.custom_results.write().unwrap();
            *custom = Some(results);
        }
        self
    }

    /// Get the current number of documents.
    pub fn len(&self) -> usize {
        self.documents.read().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.documents.read().unwrap().is_empty()
    }

    /// Get all documents (for test assertions).
    pub fn all_documents(&self) -> Vec<SearchDocument> {
        self.documents.read().unwrap().values().cloned().collect()
    }
}

#[async_trait]
impl SearchRepository for MockSearchRepository {
    async fn search(
        &self,
        query: &ParsedQuery,
        _embedding: Option<&[f32]>,
        limit: i32,
    ) -> Result<Vec<SearchResult>> {
        // Return custom results if configured
        if let Some(results) = self.custom_results.read().unwrap().as_ref() {
            return Ok(results.clone().into_iter().take(limit as usize).collect());
        }

        // Simple mock: filter documents and convert to results
        let documents = self.documents.read().unwrap();
        let mut results: Vec<SearchResult> = documents
            .values()
            .filter(|doc| {
                // Apply basic filters
                if let Some(ref source_type) = query.filters.source_type {
                    if doc.source_type != *source_type {
                        return false;
                    }
                }
                if let Some(ref project) = query.filters.project {
                    if &doc.project != project {
                        return false;
                    }
                }
                if let Some(ref priorities) = query.filters.priority {
                    match doc.priority {
                        Some(p) if priorities.contains(&p) => {}
                        _ => return false,
                    }
                }
                if let Some(ref statuses) = query.filters.status {
                    if !statuses.iter().any(|s| s.eq_ignore_ascii_case(&doc.status)) {
                        return false;
                    }
                }
                if let Some(ref item_types) = query.filters.item_type {
                    match &doc.item_type {
                        Some(t) if item_types.iter().any(|it| it.eq_ignore_ascii_case(t)) => {}
                        _ => return false,
                    }
                }
                if let Some(ref org) = query.filters.organization {
                    if &doc.organization != org {
                        return false;
                    }
                }
                if let Some(is_draft) = query.filters.is_draft {
                    if doc.is_draft != is_draft {
                        return false;
                    }
                }
                if let Some(updated_after) = query.filters.updated_after {
                    if doc.updated_at < updated_after {
                        return false;
                    }
                }

                // Simple text matching
                if !query.search_text.is_empty() {
                    let search_lower = query.search_text.to_lowercase();
                    let title_match = doc.title.to_lowercase().contains(&search_lower);
                    let desc_match = doc
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&search_lower))
                        .unwrap_or(false);
                    if !title_match && !desc_match {
                        return false;
                    }
                }

                true
            })
            .map(|doc| SearchResult {
                id: 0, // Mock doesn't have real IDs
                source_type: doc.source_type,
                source_id: doc.source_id.clone(),
                external_id: doc.external_id,
                title: doc.title.clone(),
                description: doc.description.clone(),
                status: doc.status.clone(),
                priority: doc.priority,
                item_type: doc.item_type.clone(),
                author_name: doc.author_name.clone(),
                url: doc.url.clone(),
                created_at: doc.created_at,
                updated_at: doc.updated_at,
                score: 1.0, // Mock score
            })
            .collect();

        // Sort by updated_at descending as a simple ranking
        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(results.into_iter().take(limit as usize).collect())
    }

    async fn upsert_document(&self, doc: &SearchDocument) -> Result<()> {
        let key = (doc.source_type, doc.source_id.clone());
        self.documents.write().unwrap().insert(key, doc.clone());
        Ok(())
    }

    async fn upsert_documents(&self, docs: &[SearchDocument]) -> Result<usize> {
        let mut documents = self.documents.write().unwrap();
        let mut count = 0;
        for doc in docs {
            let key = (doc.source_type, doc.source_id.clone());
            documents.insert(key, doc.clone());
            count += 1;
        }
        Ok(count)
    }

    async fn delete_document(&self, source_type: SearchSource, source_id: &str) -> Result<bool> {
        let key = (source_type, source_id.to_string());
        let removed = self.documents.write().unwrap().remove(&key);
        Ok(removed.is_some())
    }

    async fn get_document(
        &self,
        source_type: SearchSource,
        source_id: &str,
    ) -> Result<Option<SearchDocument>> {
        let key = (source_type, source_id.to_string());
        let doc = self.documents.read().unwrap().get(&key).cloned();
        Ok(doc)
    }

    async fn delete_stale_documents(&self, older_than: OffsetDateTime) -> Result<usize> {
        let mut documents = self.documents.write().unwrap();
        let stale_keys: Vec<_> = documents
            .iter()
            .filter(|(_, doc)| doc.updated_at < older_than)
            .map(|(k, _)| k.clone())
            .collect();
        let count = stale_keys.len();
        for key in stale_keys {
            documents.remove(&key);
        }
        Ok(count)
    }

    async fn count(&self, source_type: Option<SearchSource>) -> Result<i64> {
        let documents = self.documents.read().unwrap();
        let count = match source_type {
            Some(st) => documents.values().filter(|d| d.source_type == st).count(),
            None => documents.len(),
        };
        Ok(count as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::search::types::SearchFilters;

    fn make_document(source_type: SearchSource, id: &str, title: &str) -> SearchDocument {
        SearchDocument {
            source_type,
            source_id: id.to_string(),
            external_id: 1,
            title: title.to_string(),
            description: None,
            content: None,
            organization: "org".to_string(),
            project: "project".to_string(),
            repo_name: None,
            status: "active".to_string(),
            author_id: None,
            author_name: None,
            assigned_to_id: None,
            assigned_to_name: None,
            priority: None,
            item_type: None,
            is_draft: false,
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            closed_at: None,
            url: "https://example.com".to_string(),
            parent_id: None,
            linked_work_items: vec![],
            embedding: None,
        }
    }

    #[tokio::test]
    async fn upsert_and_get() {
        let repo = MockSearchRepository::new();
        let doc = make_document(SearchSource::Pr, "org/proj/repo/1", "Test PR");

        repo.upsert_document(&doc).await.unwrap();

        let retrieved = repo
            .get_document(SearchSource::Pr, "org/proj/repo/1")
            .await
            .unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test PR");
    }

    #[tokio::test]
    async fn delete_document() {
        let repo = MockSearchRepository::new();
        let doc = make_document(SearchSource::Pr, "org/proj/repo/1", "Test PR");

        repo.upsert_document(&doc).await.unwrap();
        assert_eq!(repo.len(), 1);

        let deleted = repo
            .delete_document(SearchSource::Pr, "org/proj/repo/1")
            .await
            .unwrap();
        assert!(deleted);
        assert!(repo.is_empty());
    }

    #[tokio::test]
    async fn search_filters_by_source_type() {
        let pr = make_document(SearchSource::Pr, "pr/1", "PR Title");
        let wi = make_document(SearchSource::WorkItem, "wi/1", "Work Item");

        let repo = MockSearchRepository::new().with_documents(vec![pr, wi]);

        let query = ParsedQuery {
            search_text: String::new(),
            filters: SearchFilters {
                source_type: Some(SearchSource::Pr),
                ..Default::default()
            },
        };

        let results = repo.search(&query, None, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_type, SearchSource::Pr);
    }

    #[tokio::test]
    async fn search_filters_by_text() {
        let doc1 = make_document(SearchSource::Pr, "pr/1", "Authentication fix");
        let doc2 = make_document(SearchSource::Pr, "pr/2", "Database migration");

        let repo = MockSearchRepository::new().with_documents(vec![doc1, doc2]);

        let query = ParsedQuery {
            search_text: "auth".to_string(),
            filters: SearchFilters::default(),
        };

        let results = repo.search(&query, None, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("Authentication"));
    }

    #[tokio::test]
    async fn count_by_source_type() {
        let pr1 = make_document(SearchSource::Pr, "pr/1", "PR 1");
        let pr2 = make_document(SearchSource::Pr, "pr/2", "PR 2");
        let wi = make_document(SearchSource::WorkItem, "wi/1", "Work Item");

        let repo = MockSearchRepository::new().with_documents(vec![pr1, pr2, wi]);

        assert_eq!(repo.count(None).await.unwrap(), 3);
        assert_eq!(repo.count(Some(SearchSource::Pr)).await.unwrap(), 2);
        assert_eq!(repo.count(Some(SearchSource::WorkItem)).await.unwrap(), 1);
    }
}
