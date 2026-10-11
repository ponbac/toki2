-- AI token usage uploaded by `token-ledger sync` (payload version 1).
-- Each machine belongs to the user who first uploaded from it. Buckets hold one
-- session's usage of one model during one UTC hour. An upload replaces a machine's
-- usage per provider: only providers it covers as `ok` or `partial`, and only inside
-- its window. Costs are API-equivalent USD estimates; a NULL cost is unknown, never zero.
-- Uploaded text is at most 512 characters.

CREATE TABLE ai_usage_machines
(
    id             UUID        PRIMARY KEY,
    user_id        INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    label          TEXT        NOT NULL,
    client_version TEXT        NOT NULL,
    -- The client's IANA zone, for display only; every stored instant is UTC.
    time_zone      TEXT        NOT NULL,
    first_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ai_usage_machines_id_user_id_key UNIQUE (id, user_id),
    CONSTRAINT ai_usage_machines_label_check CHECK (char_length(label) BETWEEN 1 AND 512),
    CONSTRAINT ai_usage_machines_client_version_check
        CHECK (char_length(client_version) BETWEEN 1 AND 512),
    CONSTRAINT ai_usage_machines_time_zone_check CHECK (char_length(time_zone) BETWEEN 1 AND 512)
);

CREATE INDEX ai_usage_machines_user_id_idx ON ai_usage_machines (user_id);

-- The latest coverage each provider reported from a machine, and for which window.
-- A provider keeps its row until an upload reports it again. Pricing describes the
-- stored costs, so it changes only when an upload replaces the provider's usage.
CREATE TABLE ai_usage_machine_coverage
(
    machine_id         UUID        NOT NULL REFERENCES ai_usage_machines (id) ON DELETE CASCADE,
    provider           TEXT        NOT NULL,
    status             TEXT        NOT NULL,
    files              BIGINT      NOT NULL,
    -- Existing history paths that could not be read.
    unreadable         BIGINT      NOT NULL,
    malformed_lines    BIGINT      NOT NULL,
    skipped_records    BIGINT      NOT NULL,
    duplicates         BIGINT      NOT NULL,
    window_start       TIMESTAMPTZ NOT NULL,
    window_end         TIMESTAMPTZ NOT NULL,
    reported_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL until an upload first replaces the provider's usage.
    pricing_status     TEXT,
    pricing_fetched_at TIMESTAMPTZ,
    pricing_source     TEXT,
    CONSTRAINT ai_usage_machine_coverage_pkey PRIMARY KEY (machine_id, provider),
    CONSTRAINT ai_usage_machine_coverage_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot')),
    CONSTRAINT ai_usage_machine_coverage_status_check
        CHECK (status IN ('ok', 'partial', 'failed', 'missing')),
    CONSTRAINT ai_usage_machine_coverage_counts_check CHECK (
        files >= 0 AND unreadable >= 0 AND malformed_lines >= 0 AND skipped_records >= 0
            AND duplicates >= 0
        ),
    CONSTRAINT ai_usage_machine_coverage_window_check CHECK (window_start < window_end),
    CONSTRAINT ai_usage_machine_coverage_pricing_status_check
        CHECK (pricing_status IN ('fresh', 'cached', 'unavailable', 'custom')),
    CONSTRAINT ai_usage_machine_coverage_pricing_check CHECK (
        (pricing_status IS NULL) = (pricing_source IS NULL)
            AND (pricing_status IS NOT NULL OR pricing_fetched_at IS NULL)
            AND char_length(pricing_source) <= 512
        )
);

CREATE TABLE ai_usage_buckets
(
    user_id            INTEGER     NOT NULL,
    machine_id         UUID        NOT NULL,
    hour_start         TIMESTAMPTZ NOT NULL,
    session_key        TEXT        NOT NULL,
    project_key        TEXT        NOT NULL,
    provider           TEXT        NOT NULL,
    model              TEXT        NOT NULL,
    input_tokens       BIGINT      NOT NULL,
    cache_read_tokens  BIGINT      NOT NULL,
    cache_write_tokens BIGINT      NOT NULL,
    output_tokens      BIGINT      NOT NULL,
    records            BIGINT      NOT NULL,
    -- NUMERIC keeps sums exact and independent of aggregation order.
    estimated_cost_usd NUMERIC,
    unpriced_records   BIGINT      NOT NULL,
    -- A bucket always belongs to its machine's owner.
    CONSTRAINT ai_usage_buckets_machine_fkey FOREIGN KEY (machine_id, user_id)
        REFERENCES ai_usage_machines (id, user_id) ON DELETE CASCADE,
    CONSTRAINT ai_usage_buckets_hour_start_check
        CHECK (date_trunc('hour', hour_start AT TIME ZONE 'UTC') = hour_start AT TIME ZONE 'UTC'),
    CONSTRAINT ai_usage_buckets_session_key_check CHECK (session_key ~ '^[0-9a-f]{32}$'),
    CONSTRAINT ai_usage_buckets_project_key_check
        CHECK (char_length(project_key) BETWEEN 1 AND 512),
    CONSTRAINT ai_usage_buckets_model_check CHECK (char_length(model) <= 512),
    CONSTRAINT ai_usage_buckets_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot')),
    CONSTRAINT ai_usage_buckets_tokens_check CHECK (
        input_tokens >= 0 AND cache_read_tokens >= 0 AND cache_write_tokens >= 0
            AND output_tokens >= 0
        ),
    CONSTRAINT ai_usage_buckets_records_check
        CHECK (records >= 1 AND unpriced_records >= 0 AND unpriced_records <= records),
    -- A cost is known exactly when every record in the bucket was priced.
    CONSTRAINT ai_usage_buckets_cost_check CHECK (
        estimated_cost_usd >= 0 AND (estimated_cost_usd IS NULL) = (unpriced_records > 0)
        )
);

-- The bucket key (machine_id, hour_start, session_key, project_key, provider, model)
-- is unique. The project and model are indexed by digest because two 512-character
-- values can exceed the B-tree entry limit.
CREATE UNIQUE INDEX ai_usage_buckets_key_idx ON ai_usage_buckets (
    machine_id, hour_start, provider, session_key, md5(project_key), md5(model)
);

CREATE INDEX ai_usage_buckets_user_id_hour_start_idx ON ai_usage_buckets (user_id, hour_start);

-- Billing plans a provider reported during an hour, such as Codex `plan_type`.
-- Evidence only: declared subscriptions remain authoritative.
CREATE TABLE ai_usage_provider_hints
(
    user_id    INTEGER     NOT NULL,
    machine_id UUID        NOT NULL,
    hour_start TIMESTAMPTZ NOT NULL,
    provider   TEXT        NOT NULL,
    plan       TEXT        NOT NULL,
    CONSTRAINT ai_usage_provider_hints_pkey PRIMARY KEY (machine_id, hour_start, provider, plan),
    CONSTRAINT ai_usage_provider_hints_machine_fkey FOREIGN KEY (machine_id, user_id)
        REFERENCES ai_usage_machines (id, user_id) ON DELETE CASCADE,
    CONSTRAINT ai_usage_provider_hints_hour_start_check
        CHECK (date_trunc('hour', hour_start AT TIME ZONE 'UTC') = hour_start AT TIME ZONE 'UTC'),
    CONSTRAINT ai_usage_provider_hints_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot')),
    CONSTRAINT ai_usage_provider_hints_plan_check CHECK (char_length(plan) BETWEEN 1 AND 512)
);

CREATE INDEX ai_usage_provider_hints_user_id_hour_start_idx
    ON ai_usage_provider_hints (user_id, hour_start);
