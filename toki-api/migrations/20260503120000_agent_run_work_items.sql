CREATE TABLE agent_run_work_items (
    run_id TEXT PRIMARY KEY,

    source_provider TEXT NOT NULL,
    source_organization TEXT NOT NULL,
    source_project TEXT NOT NULL,
    source_work_item_id TEXT NOT NULL,

    target_provider TEXT NOT NULL,
    target_organization TEXT,
    target_project TEXT,
    target_repo_name TEXT,
    target_default_branch TEXT,

    created_by_user_id INT REFERENCES users(id) ON DELETE SET NULL,
    created_by_display_name TEXT NOT NULL,

    last_status TEXT NOT NULL,
    draft_pr_url TEXT,

    run_created_at TIMESTAMPTZ NOT NULL,
    run_updated_at TIMESTAMPTZ NOT NULL,
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_agent_run_work_items_latest
ON agent_run_work_items (
    source_provider,
    source_organization,
    source_project,
    source_work_item_id,
    run_created_at DESC
);
