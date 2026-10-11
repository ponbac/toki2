-- Project keys a developer marked as personal or internal: not client work. Their
-- own usage of the key is left out of the bill and counts for no time-tracking
-- project. A mark belongs to one developer and applies to their usage only.
-- Mapping the key team-wide deletes every mark of it: the usage then counts as
-- the mapping says, and is unassigned again if the mapping is deleted. Marks are
-- resolved when usage is read and never written into usage rows, so marking or
-- unmarking a key applies to past usage too.

CREATE TABLE ai_personal_project_keys
(
    user_id     INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    project_key TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One 512-character key and a user id fit a B-tree entry.
    CONSTRAINT ai_personal_project_keys_pkey PRIMARY KEY (user_id, project_key),
    -- The keys that can be mapped: at most 512 characters and not padded.
    CONSTRAINT ai_personal_project_keys_project_key_check CHECK (
        char_length(project_key) BETWEEN 1 AND 512
            AND project_key !~ '^[[:space:]]|[[:space:]]$'
        ),
    -- Usage without project metadata names no project to mark.
    CONSTRAINT ai_personal_project_keys_unattributed_check CHECK (project_key <> 'unattributed')
);
