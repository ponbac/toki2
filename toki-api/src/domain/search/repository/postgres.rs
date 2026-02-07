//! PostgreSQL repository implementation with pgvector support.

use async_trait::async_trait;
use pgvector::Vector;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::domain::search::traits::{Result, SearchRepository};
use crate::domain::search::types::{ParsedQuery, SearchDocument, SearchResult, SearchSource};

/// PostgreSQL-backed search repository using pgvector for similarity search.
///
/// Implements hybrid search combining:
/// - BM25 full-text search via PostgreSQL tsvector
/// - Vector similarity search via pgvector HNSW index
/// - Reciprocal Rank Fusion (RRF) for result combination
#[derive(Clone)]
pub struct PgSearchRepository {
    pool: PgPool,
}

impl PgSearchRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute hybrid BM25 + vector search with RRF fusion.
    async fn search_hybrid(
        &self,
        query: &ParsedQuery,
        embedding: &[f32],
        limit: i32,
    ) -> Result<Vec<SearchResult>> {
        let results = sqlx::query_as!(
            SearchResultRow,
            r#"
            WITH bm25_results AS (
                SELECT
                    id,
                    ts_rank_cd(search_vector, websearch_to_tsquery('english', $1)) as score,
                    ROW_NUMBER() OVER (
                        ORDER BY ts_rank_cd(search_vector, websearch_to_tsquery('english', $1)) DESC
                    ) as rank
                FROM search_documents
                WHERE ($1 = '' OR search_vector @@ websearch_to_tsquery('english', $1))
                  AND ($2::search_source IS NULL OR source_type = $2)
                  AND ($3::text IS NULL OR organization = $3)
                  AND ($4::text IS NULL OR project = $4)
                  AND ($5::text[] IS NULL OR status = ANY($5))
                  AND ($6::int[] IS NULL OR priority = ANY($6))
                  AND ($7::text[] IS NULL OR item_type = ANY($7))
                  AND ($8::bool IS NULL OR is_draft = $8)
                  AND ($9::timestamptz IS NULL OR updated_at >= $9)
                LIMIT 100
            ),
            vector_results AS (
                SELECT
                    id,
                    1 - (embedding <=> $10) as score,
                    ROW_NUMBER() OVER (
                        ORDER BY embedding <=> $10
                    ) as rank
                FROM search_documents
                WHERE embedding IS NOT NULL
                  AND ($2::search_source IS NULL OR source_type = $2)
                  AND ($3::text IS NULL OR organization = $3)
                  AND ($4::text IS NULL OR project = $4)
                  AND ($5::text[] IS NULL OR status = ANY($5))
                  AND ($6::int[] IS NULL OR priority = ANY($6))
                  AND ($7::text[] IS NULL OR item_type = ANY($7))
                  AND ($8::bool IS NULL OR is_draft = $8)
                  AND ($9::timestamptz IS NULL OR updated_at >= $9)
                LIMIT 100
            ),
            rrf_combined AS (
                SELECT
                    COALESCE(b.id, v.id) as id,
                    (COALESCE(1.0 / (60 + b.rank), 0) + COALESCE(1.0 / (60 + v.rank), 0))::float8 as rrf_score
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
            LIMIT $11
            "#,
            query.search_text,
            query.filters.source_type as Option<SearchSource>,
            query.filters.organization.as_deref(),
            query.filters.project.as_deref(),
            query.filters.status.as_deref(),
            query.filters.priority.as_deref(),
            query.filters.item_type.as_deref(),
            query.filters.is_draft,
            query.filters.updated_after,
            Vector::from(embedding.to_vec()) as Vector,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(map_result_rows(results))
    }

    /// Execute BM25-only full-text search (no vector component).
    async fn search_bm25_only(
        &self,
        query: &ParsedQuery,
        limit: i32,
    ) -> Result<Vec<SearchResult>> {
        let results = sqlx::query_as!(
            SearchResultRow,
            r#"
            SELECT
                id,
                source_type as "source_type: SearchSource",
                source_id,
                external_id,
                title,
                description,
                status,
                priority,
                item_type,
                author_name,
                url,
                created_at,
                updated_at,
                ts_rank_cd(search_vector, websearch_to_tsquery('english', $1))::float8 as score
            FROM search_documents
            WHERE ($1 = '' OR search_vector @@ websearch_to_tsquery('english', $1))
              AND ($2::search_source IS NULL OR source_type = $2)
              AND ($3::text IS NULL OR organization = $3)
              AND ($4::text IS NULL OR project = $4)
              AND ($5::text[] IS NULL OR status = ANY($5))
              AND ($6::int[] IS NULL OR priority = ANY($6))
              AND ($7::text[] IS NULL OR item_type = ANY($7))
              AND ($8::bool IS NULL OR is_draft = $8)
              AND ($9::timestamptz IS NULL OR updated_at >= $9)
            ORDER BY score DESC
            LIMIT $10
            "#,
            query.search_text,
            query.filters.source_type as Option<SearchSource>,
            query.filters.organization.as_deref(),
            query.filters.project.as_deref(),
            query.filters.status.as_deref(),
            query.filters.priority.as_deref(),
            query.filters.item_type.as_deref(),
            query.filters.is_draft,
            query.filters.updated_after,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(map_result_rows(results))
    }
}

fn map_result_rows(rows: Vec<SearchResultRow>) -> Vec<SearchResult> {
    rows.into_iter()
        .map(|row| SearchResult {
            id: row.id,
            source_type: row.source_type,
            source_id: row.source_id,
            external_id: row.external_id,
            title: row.title,
            description: row.description,
            status: row.status,
            priority: row.priority,
            item_type: row.item_type,
            author_name: row.author_name,
            url: row.url,
            created_at: row.created_at,
            updated_at: row.updated_at,
            score: row.score.unwrap_or(0.0),
        })
        .collect()
}

/// Convert an embedding Option<Vec<f32>> to Option<Vector> without unnecessary cloning.
fn to_pg_vector(embedding: &Option<Vec<f32>>) -> Option<Vector> {
    embedding.as_deref().map(|e| Vector::from(e.to_vec()))
}

#[async_trait]
impl SearchRepository for PgSearchRepository {
    async fn search(
        &self,
        query: &ParsedQuery,
        embedding: Option<&[f32]>,
        limit: i32,
    ) -> Result<Vec<SearchResult>> {
        match embedding {
            Some(emb) => self.search_hybrid(query, emb, limit).await,
            None => self.search_bm25_only(query, limit).await,
        }
    }

    async fn upsert_document(&self, doc: &SearchDocument) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO search_documents (
                source_type, source_id, external_id, title, description, content,
                organization, project, repo_name, status,
                author_id, author_name, assigned_to_id, assigned_to_name,
                priority, item_type, is_draft,
                created_at, updated_at, closed_at,
                url, parent_id, linked_work_items, embedding
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24
            )
            ON CONFLICT (source_type, source_id) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                content = EXCLUDED.content,
                status = EXCLUDED.status,
                author_id = EXCLUDED.author_id,
                author_name = EXCLUDED.author_name,
                assigned_to_id = EXCLUDED.assigned_to_id,
                assigned_to_name = EXCLUDED.assigned_to_name,
                priority = EXCLUDED.priority,
                item_type = EXCLUDED.item_type,
                is_draft = EXCLUDED.is_draft,
                updated_at = EXCLUDED.updated_at,
                closed_at = EXCLUDED.closed_at,
                linked_work_items = EXCLUDED.linked_work_items,
                embedding = EXCLUDED.embedding,
                indexed_at = NOW()
            "#,
            doc.source_type as SearchSource,
            doc.source_id,
            doc.external_id,
            doc.title,
            doc.description.as_deref(),
            doc.content.as_deref(),
            doc.organization,
            doc.project,
            doc.repo_name.as_deref(),
            doc.status,
            doc.author_id.as_deref(),
            doc.author_name.as_deref(),
            doc.assigned_to_id.as_deref(),
            doc.assigned_to_name.as_deref(),
            doc.priority,
            doc.item_type.as_deref(),
            doc.is_draft,
            doc.created_at,
            doc.updated_at,
            doc.closed_at,
            doc.url,
            doc.parent_id,
            &doc.linked_work_items,
            to_pg_vector(&doc.embedding) as Option<Vector>,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn upsert_documents(&self, docs: &[SearchDocument]) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut count = 0;

        for doc in docs {
            sqlx::query!(
                r#"
                INSERT INTO search_documents (
                    source_type, source_id, external_id, title, description, content,
                    organization, project, repo_name, status,
                    author_id, author_name, assigned_to_id, assigned_to_name,
                    priority, item_type, is_draft,
                    created_at, updated_at, closed_at,
                    url, parent_id, linked_work_items, embedding
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                    $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                    $21, $22, $23, $24
                )
                ON CONFLICT (source_type, source_id) DO UPDATE SET
                    title = EXCLUDED.title,
                    description = EXCLUDED.description,
                    content = EXCLUDED.content,
                    status = EXCLUDED.status,
                    author_id = EXCLUDED.author_id,
                    author_name = EXCLUDED.author_name,
                    assigned_to_id = EXCLUDED.assigned_to_id,
                    assigned_to_name = EXCLUDED.assigned_to_name,
                    priority = EXCLUDED.priority,
                    item_type = EXCLUDED.item_type,
                    is_draft = EXCLUDED.is_draft,
                    updated_at = EXCLUDED.updated_at,
                    closed_at = EXCLUDED.closed_at,
                    linked_work_items = EXCLUDED.linked_work_items,
                    embedding = EXCLUDED.embedding,
                    indexed_at = NOW()
                "#,
                doc.source_type as SearchSource,
                doc.source_id,
                doc.external_id,
                doc.title,
                doc.description.as_deref(),
                doc.content.as_deref(),
                doc.organization,
                doc.project,
                doc.repo_name.as_deref(),
                doc.status,
                doc.author_id.as_deref(),
                doc.author_name.as_deref(),
                doc.assigned_to_id.as_deref(),
                doc.assigned_to_name.as_deref(),
                doc.priority,
                doc.item_type.as_deref(),
                doc.is_draft,
                doc.created_at,
                doc.updated_at,
                doc.closed_at,
                doc.url,
                doc.parent_id,
                &doc.linked_work_items,
                to_pg_vector(&doc.embedding) as Option<Vector>,
            )
            .execute(&mut *tx)
            .await?;
            count += 1;
        }

        tx.commit().await?;
        Ok(count)
    }

    async fn delete_document(&self, source_type: SearchSource, source_id: &str) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            DELETE FROM search_documents
            WHERE source_type = $1 AND source_id = $2
            "#,
            source_type as SearchSource,
            source_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_stale_documents(&self, older_than: OffsetDateTime) -> Result<usize> {
        let rows_affected = sqlx::query!(
            r#"
            DELETE FROM search_documents
            WHERE indexed_at < $1
            "#,
            older_than
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(rows_affected as usize)
    }

    async fn get_document(
        &self,
        source_type: SearchSource,
        source_id: &str,
    ) -> Result<Option<SearchDocument>> {
        let row = sqlx::query_as!(
            SearchDocumentRow,
            r#"
            SELECT
                source_type as "source_type: SearchSource",
                source_id,
                external_id,
                title,
                description,
                content,
                organization,
                project,
                repo_name,
                status,
                author_id,
                author_name,
                assigned_to_id,
                assigned_to_name,
                priority,
                item_type,
                is_draft,
                created_at,
                updated_at,
                closed_at,
                url,
                parent_id,
                linked_work_items
            FROM search_documents
            WHERE source_type = $1 AND source_id = $2
            "#,
            source_type as SearchSource,
            source_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SearchDocument {
            source_type: r.source_type,
            source_id: r.source_id,
            external_id: r.external_id,
            title: r.title,
            description: r.description,
            content: r.content,
            organization: r.organization,
            project: r.project,
            repo_name: r.repo_name,
            status: r.status,
            author_id: r.author_id,
            author_name: r.author_name,
            assigned_to_id: r.assigned_to_id,
            assigned_to_name: r.assigned_to_name,
            priority: r.priority,
            item_type: r.item_type,
            is_draft: r.is_draft.unwrap_or(false),
            created_at: r.created_at,
            updated_at: r.updated_at,
            closed_at: r.closed_at,
            url: r.url,
            parent_id: r.parent_id,
            linked_work_items: r.linked_work_items.unwrap_or_default(),
            embedding: None, // Don't fetch embedding by default (large)
        }))
    }

    async fn count(&self, source_type: Option<SearchSource>) -> Result<i64> {
        let count = match source_type {
            Some(st) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) as "count!" FROM search_documents WHERE source_type = $1"#,
                    st as SearchSource
                )
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar!(r#"SELECT COUNT(*) as "count!" FROM search_documents"#)
                    .fetch_one(&self.pool)
                    .await?
            }
        };

        Ok(count)
    }
}

// Row types for sqlx queries

#[allow(dead_code)]
struct SearchResultRow {
    id: i32,
    source_type: SearchSource,
    source_id: String,
    external_id: i32,
    title: String,
    description: Option<String>,
    status: String,
    priority: Option<i32>,
    item_type: Option<String>,
    author_name: Option<String>,
    url: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    score: Option<f64>,
}

#[allow(dead_code)]
struct SearchDocumentRow {
    source_type: SearchSource,
    source_id: String,
    external_id: i32,
    title: String,
    description: Option<String>,
    content: Option<String>,
    organization: String,
    project: String,
    repo_name: Option<String>,
    status: String,
    author_id: Option<String>,
    author_name: Option<String>,
    assigned_to_id: Option<String>,
    assigned_to_name: Option<String>,
    priority: Option<i32>,
    item_type: Option<String>,
    is_draft: Option<bool>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    closed_at: Option<OffsetDateTime>,
    url: String,
    parent_id: Option<i32>,
    linked_work_items: Option<Vec<i32>>,
}
