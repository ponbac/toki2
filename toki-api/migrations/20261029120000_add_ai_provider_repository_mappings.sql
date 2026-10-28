-- Team-wide mappings from the repositories a metered payer bills credits to (GitHub
-- Copilot's code review, from the organization's rows in ai_provider_org_usage) to
-- time-tracking projects, one per payer and repository. They point at a project
-- directly and store it as ai_project_mappings does (the project's time-tracking
-- provider and company, its id, and its name when last saved, so billing output stays
-- readable after the project is archived), so an admin can map a repository nobody has
-- mapped as a project key. They have no validity dates: they are resolved whenever a
-- bill is read and never written into usage, so a change applies retroactively, as key
-- mappings do. Only mappings to the configured company resolve; a repository without
-- one bills its credits as Unassigned (unmapped repository). Names are the payer's bare
-- repository names, compared case-insensitively.

CREATE TABLE ai_provider_repository_mappings
(
    provider               TEXT        NOT NULL,
    -- The bare repository name, as the payer spelled it when the mapping was last saved.
    repository_name        TEXT        NOT NULL,
    time_tracking_provider TEXT        NOT NULL,
    provider_company_id    TEXT        NOT NULL,
    project_id             TEXT        NOT NULL,
    project_name           TEXT        NOT NULL,
    -- The creator and the last editor; NULL once that user is deleted.
    created_by             INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by             INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ai_provider_repository_mappings_provider_check CHECK (provider IN ('copilot')),
    -- As ai_provider_org_usage.repository, never empty: credits without a repository
    -- are the IDE aggregate, which no mapping bills.
    CONSTRAINT ai_provider_repository_mappings_name_check
        CHECK (char_length(repository_name) BETWEEN 1 AND 200),
    CONSTRAINT ai_provider_repository_mappings_project_check CHECK (
        time_tracking_provider <> '' AND provider_company_id <> '' AND project_id <> '')
);

-- One mapping per repository whatever its case: the key of lookups and of upserts
-- (ON CONFLICT (provider, lower(repository_name))). It stands in for a primary key,
-- which cannot be an expression.
CREATE UNIQUE INDEX ai_provider_repository_mappings_name_key
    ON ai_provider_repository_mappings (provider, lower(repository_name));
