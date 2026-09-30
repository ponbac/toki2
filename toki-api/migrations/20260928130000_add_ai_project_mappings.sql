-- Team-wide mappings from AI usage project keys to time-tracking projects, one
-- per key. Mappings are resolved when usage is read and never written into usage
-- rows, so a change applies retroactively. Only mappings to a project of the
-- configured provider and company resolve; others are stale, and their usage is
-- unassigned like `unattributed` usage. The project name is recorded whenever the
-- mapping is saved, so billing output stays readable after the project is
-- archived.

CREATE TABLE ai_project_mappings
(
    project_key         TEXT        PRIMARY KEY,
    provider            TEXT        NOT NULL,
    provider_company_id TEXT        NOT NULL,
    project_id          TEXT        NOT NULL,
    project_name        TEXT        NOT NULL,
    -- The creator and the last editor; NULL once that user is deleted.
    created_by          INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by          INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Uploaded project keys are at most 512 characters; padded keys are not mappable.
    CONSTRAINT ai_project_mappings_project_key_check CHECK (
        char_length(project_key) BETWEEN 1 AND 512
            AND project_key !~ '^[[:space:]]|[[:space:]]$'
        ),
    -- Usage without project metadata stays unassigned.
    CONSTRAINT ai_project_mappings_unattributed_check CHECK (project_key <> 'unattributed'),
    CONSTRAINT ai_project_mappings_project_check
        CHECK (provider <> '' AND provider_company_id <> '' AND project_id <> '')
);

-- Per-key lookups of usage: whether a user's usage has a key, whether anyone's
-- does, and usage grouped by key. One 512-character key and a user id fit a
-- B-tree entry; two such keys would not.
CREATE INDEX ai_usage_buckets_project_key_user_id_idx ON ai_usage_buckets (project_key, user_id);
