//! Super Search - Hybrid semantic + full-text search over PRs and Work Items.
//!
//! This module provides a search system that combines:
//! - **BM25 full-text search** via PostgreSQL tsvector
//! - **Semantic vector search** via pgvector + Gemini embeddings
//! - **Reciprocal Rank Fusion (RRF)** for combining results
//!
//! # Architecture
//!
//! The search system is built around trait abstractions for testability:
//!
//! - [`Embedder`] - Text embedding generation (Gemini, mocks)
//! - [`SearchRepository`] - Database operations (PostgreSQL, mocks)  
//! - [`DocumentSource`] - Data fetching from Azure DevOps
//!
//! # Example
//!
//! ```ignore
//! use toki_api::domain::search::{SearchService, SearchConfig};
//! use toki_api::domain::search::embedder::GeminiEmbedder;
//! use toki_api::domain::search::repository::PgSearchRepository;
//!
//! let embedder = GeminiEmbedder::new(api_key);
//! let repository = PgSearchRepository::new(pool);
//! let service = SearchService::new(embedder, repository, SearchConfig::default());
//!
//! let results = service.search("authentication PRs", Some(20)).await?;
//! ```
//!
//! # Indexing
//!
//! Use [`SearchIndexer`] to sync data from Azure DevOps:
//!
//! ```ignore
//! use toki_api::domain::search::{SearchIndexer, IndexerConfig};
//!
//! let indexer = SearchIndexer::new(embedder, repository, ado_source, IndexerConfig::default());
//! let stats = indexer.sync_project("org", "project").await?;
//! ```
//!
//! # Query Syntax
//!
//! The search parser supports natural language queries with filter extraction:
//!
//! - `"authentication PRs"` → source_type: PR, search: "authentication"
//! - `"priority 1 bugs"` → priority: [1], item_type: ["Bug"]
//! - `"bugs in Lerum last week"` → project: "Lerums Djursjukhus", date filter
//!
//! See [`parse_query`] for full filter support.

mod index_worker;
mod indexer;
mod parser;
mod service;
mod traits;
mod types;

pub mod embedder;
pub mod repository;
pub mod source;

// Re-export main types
pub use index_worker::run_search_index_worker;
pub use indexer::{IndexerConfig, SearchIndexer};
pub use service::SearchService;
pub use types::SearchResult;
