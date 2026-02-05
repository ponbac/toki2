# Super Search Implementation Plan

## Architecture Principles

1. **Trait-based abstractions** for all external dependencies
2. **Dependency injection** via generics with trait bounds
3. **Pure business logic** separated from I/O
4. **Easy mocking** for unit tests

## Trait Definitions

### `Embedder` - Text embedding abstraction

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```

Implementations:
- `GeminiEmbedder` - Production (calls Gemini API)
- `MockEmbedder` - Tests (returns fixed vectors)

### `SearchRepository` - Database abstraction

```rust
#[async_trait]
pub trait SearchRepository: Send + Sync {
    async fn search(&self, query: &ParsedQuery, embedding: &[f32], limit: i32) -> Result<Vec<SearchResult>>;
    async fn upsert_document(&self, doc: &SearchDocument) -> Result<()>;
    async fn upsert_documents(&self, docs: &[SearchDocument]) -> Result<usize>;
    async fn delete_document(&self, source_type: SearchSource, source_id: &str) -> Result<bool>;
    async fn get_document(&self, source_type: SearchSource, source_id: &str) -> Result<Option<SearchDocument>>;
    async fn get_stale_documents(&self, older_than: OffsetDateTime) -> Result<Vec<SearchDocument>>;
}
```

Implementations:
- `PgSearchRepository` - Production (SQLx + pgvector)
- `MockSearchRepository` - Tests (in-memory HashMap)

### `DocumentSource` - ADO data fetching abstraction

```rust
#[async_trait]
pub trait DocumentSource: Send + Sync {
    async fn fetch_pull_requests(&self, org: &str, project: &str) -> Result<Vec<PullRequestDocument>>;
    async fn fetch_work_items(&self, org: &str, project: &str, since: Option<OffsetDateTime>) -> Result<Vec<WorkItemDocument>>;
}
```

Implementations:
- `AdoDocumentSource` - Production (uses existing ADO client)
- `MockDocumentSource` - Tests (returns test fixtures)

## File Structure

```
toki-api/src/domain/search/
├── mod.rs              # Module exports
├── types.rs            # SearchSource, SearchDocument, SearchResult, ParsedQuery, SearchFilters
├── traits.rs           # Embedder, SearchRepository, DocumentSource traits
├── parser.rs           # parse_query() function
├── embedder/
│   ├── mod.rs
│   ├── gemini.rs       # GeminiEmbedder
│   └── mock.rs         # MockEmbedder (cfg test)
├── repository/
│   ├── mod.rs
│   ├── postgres.rs     # PgSearchRepository
│   └── mock.rs         # MockSearchRepository (cfg test)
├── source/
│   ├── mod.rs
│   ├── ado.rs          # AdoDocumentSource
│   └── mock.rs         # MockDocumentSource (cfg test)
├── service.rs          # SearchService<E, R>
└── indexer.rs          # SearchIndexer<E, R, S>
```

## Services with Generics

### SearchService

```rust
pub struct SearchService<E, R>
where
    E: Embedder,
    R: SearchRepository,
{
    embedder: E,
    repository: R,
}

impl<E, R> SearchService<E, R>
where
    E: Embedder,
    R: SearchRepository,
{
    pub fn new(embedder: E, repository: R) -> Self {
        Self { embedder, repository }
    }

    pub async fn search(&self, query: &str, limit: i32) -> Result<Vec<SearchResult>> {
        let parsed = parse_query(query);
        let embedding = self.embedder.embed(&parsed.search_text).await?;
        self.repository.search(&parsed, &embedding, limit).await
    }
}
```

### SearchIndexer

```rust
pub struct SearchIndexer<E, R, S>
where
    E: Embedder,
    R: SearchRepository,
    S: DocumentSource,
{
    embedder: E,
    repository: R,
    source: S,
}

impl<E, R, S> SearchIndexer<E, R, S>
where
    E: Embedder,
    R: SearchRepository,
    S: DocumentSource,
{
    pub async fn sync_project(&self, org: &str, project: &str) -> Result<SyncStats> {
        // Fetch from source, embed, upsert to repository
    }
}
```

## Implementation Order

1. **Migration** - `migrations/YYYYMMDDHHMMSS_create_search_documents.sql`
2. **Types** - `types.rs` (all data structures)
3. **Traits** - `traits.rs` (all trait definitions)
4. **Parser** - `parser.rs` (query parsing, pure function, easy to test)
5. **Mock implementations** - For testing the service layer
6. **SearchService** - `service.rs` (with unit tests using mocks)
7. **Embedder** - `embedder/gemini.rs` (Gemini API client)
8. **Repository** - `repository/postgres.rs` (SQLx implementation)
9. **DocumentSource** - `source/ado.rs` (ADO client wrapper)
10. **Indexer** - `indexer.rs` (with unit tests using mocks)
11. **API Route** - `routes/search.rs`
12. **Integration tests** - Full stack with test database

## Testing Strategy

### Unit Tests (fast, no I/O)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::search::embedder::MockEmbedder;
    use crate::domain::search::repository::MockSearchRepository;

    #[tokio::test]
    async fn search_returns_results_sorted_by_score() {
        let embedder = MockEmbedder::returning(vec![0.1; 3072]);
        let repo = MockSearchRepository::new()
            .with_document(SearchDocument { title: "Auth PR".into(), .. })
            .with_document(SearchDocument { title: "Other PR".into(), .. });
        
        let service = SearchService::new(embedder, repo);
        let results = service.search("authentication", 10).await.unwrap();
        
        assert_eq!(results.len(), 2);
        assert!(results[0].score >= results[1].score);
    }

    #[tokio::test]
    async fn search_applies_filters() {
        // Test that status/priority/project filters work
    }

    #[tokio::test]
    async fn parser_extracts_priority_filter() {
        let parsed = parse_query("priority 1 bugs");
        assert_eq!(parsed.filters.priority, Some(vec![1]));
        assert_eq!(parsed.filters.item_type, Some(vec!["Bug".to_string()]));
        assert_eq!(parsed.search_text, "");
    }
}
```

### Integration Tests (with test DB)

```rust
#[sqlx::test]
async fn search_integration(pool: PgPool) {
    let embedder = GeminiEmbedder::new(test_api_key());
    let repo = PgSearchRepository::new(pool);
    let service = SearchService::new(embedder, repo);
    
    // Insert test documents, run search, verify results
}
```

## Dependencies

```toml
[dependencies]
pgvector = "0.4"
async-trait = "0.1"

[dev-dependencies]
mockall = "0.13"  # Optional: for derive-based mocking
```

## Config

```yaml
search:
  enabled: true
  gemini_api_key: ${GEMINI_API_KEY}
  embedding_model: "gemini-embedding-001"
  embedding_dimensions: 3072
  sync_interval_minutes: 15
  batch_size: 10
```
