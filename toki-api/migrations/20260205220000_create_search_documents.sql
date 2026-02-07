-- Super Search: Hybrid semantic + full-text search for PRs and Work Items
-- Requires pgvector extension for vector similarity search

CREATE EXTENSION IF NOT EXISTS vector;

-- Source type enum (idempotent: Postgres lacks CREATE TYPE IF NOT EXISTS for enums)
DO $$ BEGIN
    CREATE TYPE search_source AS ENUM ('pr', 'work_item');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- Main search documents table
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
    item_type TEXT,  -- 'Bug', 'Task', 'User Story' for WI; NULL for PRs
    is_draft BOOLEAN DEFAULT FALSE,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    closed_at TIMESTAMPTZ,
    indexed_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Full-text search vector (weighted: A=title, B=description, C=content)
    search_vector TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(description, '')), 'B') ||
        setweight(to_tsvector('english', coalesce(content, '')), 'C')
    ) STORED,
    
    -- Embedding vector for semantic search (Gemini: 1536 dimensions, reduced via outputDimensionality)
    embedding vector(1536),
    
    -- Links
    url TEXT NOT NULL,
    parent_id INT,  -- For work items with parent
    linked_work_items INT[],  -- Work items linked to a PR
    
    -- Unique constraint on source
    UNIQUE(source_type, source_id)
);

-- Indexes for search performance

-- Full-text search (GIN for tsvector)
CREATE INDEX idx_search_docs_search_vector ON search_documents USING GIN(search_vector);

-- Vector similarity search (HNSW for approximate nearest neighbor)
CREATE INDEX idx_search_docs_embedding ON search_documents USING hnsw(embedding vector_cosine_ops);

-- Metadata filtering (B-tree indexes)
CREATE INDEX idx_search_docs_org_project ON search_documents(organization, project);
CREATE INDEX idx_search_docs_source_type ON search_documents(source_type);
CREATE INDEX idx_search_docs_status ON search_documents(status);
CREATE INDEX idx_search_docs_priority ON search_documents(priority) WHERE priority IS NOT NULL;
CREATE INDEX idx_search_docs_item_type ON search_documents(item_type) WHERE item_type IS NOT NULL;
CREATE INDEX idx_search_docs_author ON search_documents(author_id) WHERE author_id IS NOT NULL;
CREATE INDEX idx_search_docs_created ON search_documents(created_at DESC);
CREATE INDEX idx_search_docs_updated ON search_documents(updated_at DESC);
CREATE INDEX idx_search_docs_indexed ON search_documents(indexed_at DESC);
