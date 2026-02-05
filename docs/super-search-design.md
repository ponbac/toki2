# Super Search Design

Hybrid search over PRs and Work Items using Gemini embeddings + Postgres full-text search.

## Goals

- **Semantic search**: "PRs about authentication" finds PRs even if they don't contain the word
- **Keyword search**: Exact matches, IDs, names
- **Metadata filtering**: Status, priority, author, date ranges, repo
- **Unified results**: PRs and work items in one search

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Search Flow                               │
└─────────────────────────────────────────────────────────────────┘

  User Query: "authentication issues in Lerum, priority 1"
                            │
                            ▼
               ┌────────────────────────┐
               │    Query Parser        │
               │  - Extract filters     │
               │  - Extract search text │
               └───────────┬────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
   ┌──────────┐     ┌──────────────┐   ┌──────────┐
   │  BM25    │     │   Vector     │   │ Metadata │
   │(tsvector)│     │   Search     │   │  Filter  │
   └────┬─────┘     └──────┬───────┘   └────┬─────┘
        │                  │                │
        └─────────────────┬┴────────────────┘
                          ▼
               ┌────────────────────────┐
               │   Reciprocal Rank      │
               │   Fusion (RRF)         │
               └───────────┬────────────┘
                           ▼
               ┌────────────────────────┐
               │   Results + Snippets   │
               └────────────────────────┘
```

## Data Model

### `search_documents` Table

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TYPE search_source AS ENUM ('pr', 'work_item');

CREATE TABLE search_documents (
    id SERIAL PRIMARY KEY,
    
    -- Source identification
    source_type search_source NOT NULL,
    source_id TEXT NOT NULL,  -- "org/project/repo/123" for PR, "org/project/123" for WI
    external_id INT NOT NULL, -- PR number or work item ID
    
    -- Searchable content
    title TEXT NOT NULL,
    description TEXT,
    content TEXT,  -- Combined: description + comments + commit messages
    
    -- Metadata (filterable)
    organization TEXT NOT NULL,
    project TEXT NOT NULL,
    repo_name TEXT,  -- NULL for work items
    status TEXT NOT NULL,  -- 'active', 'completed', 'abandoned' / 'New', 'Active', 'Closed'
    author_id TEXT,
    author_name TEXT,
    assigned_to_id TEXT,
    assigned_to_name TEXT,
    priority INT,  -- 1-4 for work items, NULL for PRs
    item_type TEXT,  -- 'Bug', 'Task', 'User Story' for WI; 'pr' for PRs
    is_draft BOOLEAN DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    closed_at TIMESTAMPTZ,
    indexed_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Search vectors
    search_vector TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(description, '')), 'B') ||
        setweight(to_tsvector('english', coalesce(content, '')), 'C')
    ) STORED,
    embedding vector(1536),  -- Gemini gemini-embedding-001 outputs 1536 dims
    
    -- Links
    url TEXT NOT NULL,
    parent_id INT,  -- For work items with parent
    linked_work_items INT[],  -- Work items linked to a PR
    
    UNIQUE(source_type, source_id)
);

-- Indexes
CREATE INDEX idx_search_docs_search_vector ON search_documents USING GIN(search_vector);
CREATE INDEX idx_search_docs_embedding ON search_documents USING hnsw(embedding vector_cosine_ops);
CREATE INDEX idx_search_docs_org_project ON search_documents(organization, project);
CREATE INDEX idx_search_docs_status ON search_documents(status);
CREATE INDEX idx_search_docs_priority ON search_documents(priority) WHERE priority IS NOT NULL;
CREATE INDEX idx_search_docs_created ON search_documents(created_at DESC);
CREATE INDEX idx_search_docs_updated ON search_documents(updated_at DESC);
```

## Gemini Embedding Integration

Using the `genai` Rust crate for clean provider abstraction:

```rust
// toki-api/src/domain/search/embedder/gemini.rs
use genai::embed::EmbedOptions;

pub const GEMINI_MODEL: &str = "gemini-embedding-001";
pub const GEMINI_DIMENSIONS: usize = 1536;

pub struct GeminiEmbedder {
    client: genai::Client,
    model: String,
    options: EmbedOptions,
}

impl GeminiEmbedder {
    pub fn new() -> Result<Self> {
        let client = genai::Client::default();
        let options = EmbedOptions::new().with_embedding_type("RETRIEVAL_QUERY");
        Ok(Self { client, model: GEMINI_MODEL.into(), options })
    }
}

#[async_trait]
impl Embedder for GeminiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let response = self.client.embed(&self.model, text, Some(&self.options)).await?;
        Ok(response.first_embedding().unwrap().vector().iter().map(|&v| v as f32).collect())
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let response = self.client.embed_batch(&self.model, texts, Some(&self.options)).await?;
        // ... map embeddings back to results
    }
}
```

Benefits of `genai` crate:
- Unified interface across providers (easy to swap to OpenAI, Cohere, etc.)
- Built-in batching with `embed_batch`
- Reads `GEMINI_API_KEY` from environment automatically
- Handles retries and rate limiting

## Search Query Parser

Parse natural language into filters + search text:

```rust
// "priority 1 bugs in Lerum closed last week"
// → filters: {priority: 1, item_type: "Bug", project: "Lerums Djursjukhus", status: "Closed", date_range: last_week}
// → search_text: ""

// "authentication PRs"  
// → filters: {source_type: "pr"}
// → search_text: "authentication"

pub struct ParsedQuery {
    pub search_text: String,
    pub filters: SearchFilters,
}

#[derive(Default)]
pub struct SearchFilters {
    pub source_type: Option<SearchSource>,
    pub organization: Option<String>,
    pub project: Option<String>,
    pub repo_name: Option<String>,
    pub status: Option<Vec<String>>,
    pub priority: Option<Vec<i32>>,
    pub item_type: Option<Vec<String>>,
    pub author: Option<String>,
    pub assigned_to: Option<String>,
    pub is_draft: Option<bool>,
    pub created_after: Option<OffsetDateTime>,
    pub created_before: Option<OffsetDateTime>,
    pub updated_after: Option<OffsetDateTime>,
}
```

## Hybrid Search Implementation

```rust
pub struct SearchService {
    pool: PgPool,
    embedder: GeminiEmbedder,
}

impl SearchService {
    pub async fn search(&self, query: &str, limit: i32) -> anyhow::Result<Vec<SearchResult>> {
        let parsed = parse_query(query);
        
        // Generate embedding for semantic search
        let query_embedding = self.embedder.embed_query(&parsed.search_text).await?;
        
        // Hybrid search with RRF (Reciprocal Rank Fusion)
        let results = sqlx::query_as!(
            SearchResult,
            r#"
            WITH bm25_results AS (
                SELECT id, 
                       ts_rank_cd(search_vector, websearch_to_tsquery('english', $1)) as score,
                       ROW_NUMBER() OVER (ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('english', $1)) DESC) as rank
                FROM search_documents
                WHERE ($1 = '' OR search_vector @@ websearch_to_tsquery('english', $1))
                  AND ($2::text IS NULL OR organization = $2)
                  AND ($3::text IS NULL OR project = $3)
                  AND ($4::text[] IS NULL OR status = ANY($4))
                  AND ($5::int[] IS NULL OR priority = ANY($5))
                  AND ($6::search_source IS NULL OR source_type = $6)
                LIMIT 100
            ),
            vector_results AS (
                SELECT id,
                       1 - (embedding <=> $7::vector) as score,
                       ROW_NUMBER() OVER (ORDER BY embedding <=> $7::vector) as rank
                FROM search_documents
                WHERE embedding IS NOT NULL
                  AND ($2::text IS NULL OR organization = $2)
                  AND ($3::text IS NULL OR project = $3)
                  AND ($4::text[] IS NULL OR status = ANY($4))
                  AND ($5::int[] IS NULL OR priority = ANY($5))
                  AND ($6::search_source IS NULL OR source_type = $6)
                LIMIT 100
            ),
            rrf_combined AS (
                SELECT 
                    COALESCE(b.id, v.id) as id,
                    COALESCE(1.0 / (60 + b.rank), 0) + COALESCE(1.0 / (60 + v.rank), 0) as rrf_score
                FROM bm25_results b
                FULL OUTER JOIN vector_results v ON b.id = v.id
            )
            SELECT 
                d.id,
                d.source_type as "source_type: SearchSource",
                d.source_id,
                d.external_id,
                d.title,
                d.description,
                d.status,
                d.priority,
                d.item_type,
                d.author_name,
                d.url,
                d.created_at,
                d.updated_at,
                r.rrf_score as score
            FROM rrf_combined r
            JOIN search_documents d ON d.id = r.id
            ORDER BY r.rrf_score DESC
            LIMIT $8
            "#,
            parsed.search_text,
            parsed.filters.organization,
            parsed.filters.project,
            parsed.filters.status.as_deref(),
            parsed.filters.priority.as_deref(),
            parsed.filters.source_type as Option<SearchSource>,
            &query_embedding as &[f32],
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(results)
    }
}
```

## Sync Service

Background job to sync from Azure DevOps to search index:

```rust
pub struct SearchIndexer {
    pool: PgPool,
    embedder: GeminiEmbedder,
    ado_client: AzureDevOpsClient,
}

impl SearchIndexer {
    /// Full sync - run periodically or on demand
    pub async fn sync_all(&self, org: &str, project: &str) -> anyhow::Result<SyncStats> {
        let mut stats = SyncStats::default();
        
        // Sync PRs (active + recently closed)
        let prs = self.ado_client.list_pull_requests(org, project, PrStatus::All).await?;
        for pr in prs {
            self.index_pull_request(&pr).await?;
            stats.prs_indexed += 1;
        }
        
        // Sync work items (query for recent changes)
        let work_items = self.ado_client.query_work_items(org, project, 
            "SELECT [System.Id] FROM WorkItems WHERE [System.ChangedDate] >= @Today - 30"
        ).await?;
        for wi in work_items {
            self.index_work_item(org, project, &wi).await?;
            stats.work_items_indexed += 1;
        }
        
        Ok(stats)
    }
    
    async fn index_pull_request(&self, pr: &PullRequest) -> anyhow::Result<()> {
        // Combine content for embedding
        let content = format!(
            "{}\n\n{}\n\nCommits:\n{}\n\nComments:\n{}",
            pr.pull_request_base.title,
            pr.pull_request_base.description.as_deref().unwrap_or(""),
            pr.commits.iter().map(|c| &c.comment).collect::<Vec<_>>().join("\n"),
            pr.threads.iter()
                .flat_map(|t| t.comments.iter())
                .filter(|c| !c.is_system_comment())
                .map(|c| c.content.as_deref().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
        );
        
        let embedding = self.embedder.embed(&content).await?;
        
        sqlx::query!(
            r#"
            INSERT INTO search_documents (
                source_type, source_id, external_id, title, description, content,
                organization, project, repo_name, status, author_id, author_name,
                is_draft, created_at, updated_at, url, linked_work_items, embedding
            ) VALUES (
                'pr', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
            )
            ON CONFLICT (source_type, source_id) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                content = EXCLUDED.content,
                status = EXCLUDED.status,
                is_draft = EXCLUDED.is_draft,
                updated_at = EXCLUDED.updated_at,
                linked_work_items = EXCLUDED.linked_work_items,
                embedding = EXCLUDED.embedding,
                indexed_at = NOW()
            "#,
            // ... parameters
        ).execute(&self.pool).await?;
        
        Ok(())
    }
}
```

## API Endpoints

```rust
// GET /api/search?q=authentication+PRs&limit=20
// GET /api/search?q=priority+1+bugs&project=Lerums+Djursjukhus

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i32,
    // Optional explicit filters (override parsed)
    project: Option<String>,
    status: Option<String>,
    priority: Option<i32>,
    source_type: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, AppError> {
    let results = state.search_service.search(&params.q, params.limit).await?;
    Ok(Json(SearchResponse { results }))
}
```

## Frontend Component

```tsx
// app/src/components/SuperSearch.tsx
function SuperSearch() {
  const [query, setQuery] = useState('')
  const { data, isLoading } = useQuery({
    queryKey: ['search', query],
    queryFn: () => api.search(query),
    enabled: query.length > 2,
  })

  return (
    <Command>
      <CommandInput 
        placeholder="Search PRs and work items..." 
        value={query}
        onValueChange={setQuery}
      />
      <CommandList>
        {data?.results.map((result) => (
          <CommandItem key={result.id}>
            <ResultCard result={result} />
          </CommandItem>
        ))}
      </CommandList>
    </Command>
  )
}
```

## Implementation Plan

1. **Database setup** (1h)
   - Add pgvector extension
   - Create migration for `search_documents` table

2. **Gemini embeddings** (2h)
   - Add `GEMINI_API_KEY` to config
   - Implement embedder client

3. **Indexer service** (4h)
   - PR indexing with full content
   - Work item indexing
   - Background sync job

4. **Search service** (3h)
   - Query parser
   - Hybrid search with RRF
   - Result formatting

5. **API endpoint** (1h)
   - Search route
   - Response types

6. **Frontend** (3h)
   - Command palette component
   - Result cards with metadata
   - Filters UI

## Config

```yaml
# config/base.yaml
search:
  enabled: true
  gemini_api_key: ${GEMINI_API_KEY}
  sync_interval_minutes: 15
  embedding_batch_size: 10
```

## Future Enhancements

- **Query suggestions** - Autocomplete based on recent searches
- **Saved searches** - Pin common queries
- **Search within PR** - Drill down into specific PR content
- **AI summaries** - Generate summary of search results using Gemini
- **Related items** - "Similar PRs" based on embedding distance
