-- What a metered payer billed (GitHub Copilot, from the organization's billing usage API),
-- imported per UTC day: each account's credits per product, SKU and model label
-- (ai_provider_account_usage), the organization's credits per model (ai_provider_org_credits,
-- for the per-day residual: org total minus the per-account sum, "unknown account"), the
-- organization's rows per SKU and repository (ai_provider_org_usage: code review credits, cloud
-- agent, Actions, seats; an empty repository on AI credits is the IDE aggregate, never summed
-- with per-account rows), and the month summary (ai_provider_usage_summaries: seats and the pool
-- discount). Quantities, unit prices and gross/discount/net are stored exactly as GitHub gives
-- them. gross_usd_nanos is G: gross quantity x $0.01, rounded half up to nanodollars at import.
-- Every fetched slice (an account's day, the organization's day, a month's summary) is replaced
-- in one transaction, so rows GitHub restates under another label never linger; rows whose
-- content is unchanged are left alone, so re-imports change nothing.
-- ai_provider_usage_slices records each slice's fetches, for coverage and the settle lag. Keys
-- have no NULLs: an empty label or repository is ''. Only accounts in ai_provider_accounts are
-- imported; usage of accounts Toki never listed is the residual. An advisory lock keeps two
-- imports apart.

CREATE TABLE ai_provider_account_usage
(
    provider          TEXT        NOT NULL,
    account_id        BIGINT      NOT NULL,
    day               DATE        NOT NULL,
    product           TEXT        NOT NULL,
    sku               TEXT        NOT NULL,
    -- The payer's label as given, such as 'Auto: GPT-6.1 Sol'.
    model_label       TEXT        NOT NULL,
    -- The label in the naming of ai_usage_buckets.model, such as 'gpt-6.1-sol'.
    model             TEXT        NOT NULL,
    auto_routed       BOOLEAN     NOT NULL,
    kind              TEXT        NOT NULL,
    unit_type         TEXT        NOT NULL,
    unit_price        NUMERIC     NOT NULL,
    gross_quantity    NUMERIC     NOT NULL,
    gross_amount      NUMERIC     NOT NULL,
    discount_quantity NUMERIC     NOT NULL,
    discount_amount   NUMERIC     NOT NULL,
    net_quantity      NUMERIC     NOT NULL,
    net_amount        NUMERIC     NOT NULL,
    gross_usd_nanos   BIGINT      NOT NULL,
    -- The login the rows were requested for.
    login             TEXT        NOT NULL,
    -- The fetch whose content these rows are; unchanged by re-fetches of equal content.
    fetched_at        TIMESTAMPTZ NOT NULL,
    CONSTRAINT ai_provider_account_usage_pkey
        PRIMARY KEY (provider, account_id, day, product, sku, model_label),
    CONSTRAINT ai_provider_account_usage_account_fkey FOREIGN KEY (provider, account_id)
        REFERENCES ai_provider_accounts (provider, account_id),
    CONSTRAINT ai_provider_account_usage_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_account_usage_kind_check
        CHECK (kind IN ('trackable', 'untrackable', 'repository_billed', 'unrecognized')),
    CONSTRAINT ai_provider_account_usage_text_check CHECK (
        char_length(product) <= 200 AND char_length(sku) <= 200 AND char_length(model_label) <= 200
            AND char_length(model) <= 200 AND char_length(unit_type) <= 100
            AND char_length(login) BETWEEN 1 AND 100),
    CONSTRAINT ai_provider_account_usage_model_check CHECK (model <> '' OR kind = 'unrecognized'),
    CONSTRAINT ai_provider_account_usage_amounts_check CHECK (
        unit_price >= 0 AND gross_quantity >= 0 AND gross_amount >= 0 AND discount_quantity >= 0
            AND discount_amount >= 0 AND net_quantity >= 0 AND net_amount >= 0
            AND gross_usd_nanos >= 0)
);

CREATE INDEX ai_provider_account_usage_day_idx ON ai_provider_account_usage (provider, day);

CREATE TABLE ai_provider_org_credits
(
    provider          TEXT        NOT NULL,
    day               DATE        NOT NULL,
    product           TEXT        NOT NULL,
    sku               TEXT        NOT NULL,
    model_label       TEXT        NOT NULL,
    model             TEXT        NOT NULL,
    auto_routed       BOOLEAN     NOT NULL,
    kind              TEXT        NOT NULL,
    unit_type         TEXT        NOT NULL,
    unit_price        NUMERIC     NOT NULL,
    gross_quantity    NUMERIC     NOT NULL,
    gross_amount      NUMERIC     NOT NULL,
    discount_quantity NUMERIC     NOT NULL,
    discount_amount   NUMERIC     NOT NULL,
    net_quantity      NUMERIC     NOT NULL,
    net_amount        NUMERIC     NOT NULL,
    gross_usd_nanos   BIGINT      NOT NULL,
    fetched_at        TIMESTAMPTZ NOT NULL,
    CONSTRAINT ai_provider_org_credits_pkey PRIMARY KEY (provider, day, product, sku, model_label),
    CONSTRAINT ai_provider_org_credits_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_org_credits_kind_check
        CHECK (kind IN ('trackable', 'untrackable', 'repository_billed', 'unrecognized')),
    CONSTRAINT ai_provider_org_credits_text_check CHECK (
        char_length(product) <= 200 AND char_length(sku) <= 200 AND char_length(model_label) <= 200
            AND char_length(model) <= 200 AND char_length(unit_type) <= 100),
    CONSTRAINT ai_provider_org_credits_model_check CHECK (model <> '' OR kind = 'unrecognized'),
    CONSTRAINT ai_provider_org_credits_amounts_check CHECK (
        unit_price >= 0 AND gross_quantity >= 0 AND gross_amount >= 0 AND discount_quantity >= 0
            AND discount_amount >= 0 AND net_quantity >= 0 AND net_amount >= 0
            AND gross_usd_nanos >= 0)
);

CREATE TABLE ai_provider_org_usage
(
    provider         TEXT        NOT NULL,
    day              DATE        NOT NULL,
    product          TEXT        NOT NULL,
    sku              TEXT        NOT NULL,
    unit_type        TEXT        NOT NULL,
    -- The payer's bare repositoryName, without its owner; '' when the row names none.
    repository       TEXT        NOT NULL,
    kind             TEXT        NOT NULL,
    unit_price       NUMERIC     NOT NULL,
    quantity         NUMERIC     NOT NULL,
    gross_amount     NUMERIC     NOT NULL,
    discount_amount  NUMERIC     NOT NULL,
    net_amount       NUMERIC     NOT NULL,
    -- quantity x $0.01 for credit rows, rounded half up; NULL for minutes, storage and seats.
    credit_usd_nanos BIGINT,
    -- The row's own `date` instant (the earliest, when several rows were merged).
    reported_at      TIMESTAMPTZ,
    fetched_at       TIMESTAMPTZ NOT NULL,
    CONSTRAINT ai_provider_org_usage_pkey
        PRIMARY KEY (provider, day, product, sku, unit_type, repository),
    CONSTRAINT ai_provider_org_usage_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_org_usage_kind_check CHECK (kind IN (
        'repository_credits', 'account_credits_aggregate', 'untrackable_credits', 'compute', 'seats',
        'unrecognized')),
    CONSTRAINT ai_provider_org_usage_repository_check CHECK (
        (kind <> 'repository_credits' OR repository <> '')
            AND (kind <> 'account_credits_aggregate' OR repository = '')),
    CONSTRAINT ai_provider_org_usage_credits_check CHECK (
        (credit_usd_nanos IS NOT NULL)
            = (kind IN ('repository_credits', 'account_credits_aggregate', 'untrackable_credits'))
            AND (credit_usd_nanos IS NULL OR credit_usd_nanos >= 0)),
    CONSTRAINT ai_provider_org_usage_text_check CHECK (
        char_length(product) <= 200 AND char_length(sku) <= 200 AND char_length(unit_type) <= 100
            AND char_length(repository) <= 200),
    CONSTRAINT ai_provider_org_usage_amounts_check CHECK (
        unit_price >= 0 AND quantity >= 0 AND gross_amount >= 0 AND discount_amount >= 0
            AND net_amount >= 0)
);

CREATE TABLE ai_provider_usage_summaries
(
    provider          TEXT        NOT NULL,
    -- The month's first day.
    month             DATE        NOT NULL,
    product           TEXT        NOT NULL,
    sku               TEXT        NOT NULL,
    unit_type         TEXT        NOT NULL,
    kind              TEXT        NOT NULL,
    unit_price        NUMERIC     NOT NULL,
    gross_quantity    NUMERIC     NOT NULL,
    gross_amount      NUMERIC     NOT NULL,
    discount_quantity NUMERIC     NOT NULL,
    discount_amount   NUMERIC     NOT NULL,
    net_quantity      NUMERIC     NOT NULL,
    net_amount        NUMERIC     NOT NULL,
    fetched_at        TIMESTAMPTZ NOT NULL,
    CONSTRAINT ai_provider_usage_summaries_pkey PRIMARY KEY (provider, month, product, sku, unit_type),
    CONSTRAINT ai_provider_usage_summaries_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_usage_summaries_month_check CHECK (extract(day FROM month) = 1),
    CONSTRAINT ai_provider_usage_summaries_kind_check
        CHECK (kind IN ('credits', 'untrackable_credits', 'seats', 'unrecognized')),
    CONSTRAINT ai_provider_usage_summaries_text_check CHECK (
        char_length(product) <= 200 AND char_length(sku) <= 200 AND char_length(unit_type) <= 100),
    CONSTRAINT ai_provider_usage_summaries_amounts_check CHECK (
        unit_price >= 0 AND gross_quantity >= 0 AND gross_amount >= 0 AND discount_quantity >= 0
            AND discount_amount >= 0 AND net_quantity >= 0 AND net_amount >= 0)
);

-- One row per fetched slice: an account's day (account_credits), the organization's day
-- (org_credits, org_usage; account_id 0), or a month's summary (day = the month's first).
-- changed_at is the latest fetch that found other content than the stored one; complete_at the
-- first fetch after the slice's period ended; late_changed_at the latest fetch that found other
-- content although the fetch before it was already after the period ended (the settle lag).
CREATE TABLE ai_provider_usage_slices
(
    provider        TEXT        NOT NULL,
    source          TEXT        NOT NULL,
    account_id      BIGINT      NOT NULL,
    day             DATE        NOT NULL,
    fetched_at      TIMESTAMPTZ NOT NULL,
    changed_at      TIMESTAMPTZ NOT NULL,
    complete_at     TIMESTAMPTZ,
    late_changed_at TIMESTAMPTZ,
    CONSTRAINT ai_provider_usage_slices_pkey PRIMARY KEY (provider, source, account_id, day),
    CONSTRAINT ai_provider_usage_slices_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_usage_slices_source_check
        CHECK (source IN ('account_credits', 'org_credits', 'org_usage', 'summary')),
    CONSTRAINT ai_provider_usage_slices_account_check
        CHECK (account_id >= 0 AND (source = 'account_credits') = (account_id > 0)),
    CONSTRAINT ai_provider_usage_slices_summary_check
        CHECK (source <> 'summary' OR extract(day FROM day) = 1),
    CONSTRAINT ai_provider_usage_slices_times_check CHECK (
        changed_at <= fetched_at AND (complete_at IS NULL OR complete_at <= fetched_at)
            AND (late_changed_at IS NULL OR late_changed_at <= fetched_at))
);

-- The latest comparison of the payer's month total with the stored days, per account
-- (account_credits) or for the organization (org_credits, account_id 0).
CREATE TABLE ai_provider_usage_checks
(
    provider              TEXT        NOT NULL,
    source                TEXT        NOT NULL,
    account_id            BIGINT      NOT NULL,
    month                 DATE        NOT NULL,
    checked_at            TIMESTAMPTZ NOT NULL,
    payer_gross_quantity NUMERIC,
    stored_gross_quantity NUMERIC     NOT NULL,
    -- Every (product, sku, label) total within 0.000001 credits.
    matches               BOOLEAN,
    error                 TEXT,
    CONSTRAINT ai_provider_usage_checks_pkey PRIMARY KEY (provider, source, account_id, month),
    CONSTRAINT ai_provider_usage_checks_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_usage_checks_source_check
        CHECK (source IN ('account_credits', 'org_credits')),
    CONSTRAINT ai_provider_usage_checks_account_check
        CHECK (account_id >= 0 AND (source = 'account_credits') = (account_id > 0)),
    CONSTRAINT ai_provider_usage_checks_month_check CHECK (extract(day FROM month) = 1),
    CONSTRAINT ai_provider_usage_checks_outcome_check
        CHECK ((error IS NULL) = (payer_gross_quantity IS NOT NULL AND matches IS NOT NULL)),
    CONSTRAINT ai_provider_usage_checks_quantities_check
        CHECK (stored_gross_quantity >= 0 AND (payer_gross_quantity IS NULL OR payer_gross_quantity >= 0)),
    CONSTRAINT ai_provider_usage_checks_error_check CHECK (char_length(error) BETWEEN 1 AND 2000)
);

CREATE TABLE ai_provider_import_runs
(
    id             BIGSERIAL   PRIMARY KEY,
    provider       TEXT        NOT NULL,
    kind           TEXT        NOT NULL,
    requested_by   INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    -- The months imported (first days), and those refreshed in full.
    months         DATE[]      NOT NULL,
    full_months    DATE[]      NOT NULL,
    started_at     TIMESTAMPTZ NOT NULL,
    finished_at    TIMESTAMPTZ,
    status         TEXT        NOT NULL,
    requests       INTEGER     NOT NULL DEFAULT 0,
    slices_fetched INTEGER     NOT NULL DEFAULT 0,
    slices_changed INTEGER     NOT NULL DEFAULT 0,
    account_errors INTEGER     NOT NULL DEFAULT 0,
    error          TEXT,
    CONSTRAINT ai_provider_import_runs_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_import_runs_kind_check CHECK (kind IN ('hourly', 'nightly', 'admin')),
    CONSTRAINT ai_provider_import_runs_requested_check CHECK (kind = 'admin' OR requested_by IS NULL),
    CONSTRAINT ai_provider_import_runs_status_check
        CHECK (status IN ('running', 'succeeded', 'partial', 'failed')),
    CONSTRAINT ai_provider_import_runs_finished_check CHECK (
        (status = 'running') = (finished_at IS NULL)
            AND (finished_at IS NULL OR started_at <= finished_at)),
    CONSTRAINT ai_provider_import_runs_counts_check CHECK (
        requests >= 0 AND slices_fetched >= 0 AND slices_changed >= 0 AND account_errors >= 0),
    CONSTRAINT ai_provider_import_runs_error_check CHECK (char_length(error) BETWEEN 1 AND 2000)
);

CREATE INDEX ai_provider_import_runs_started_idx ON ai_provider_import_runs (provider, started_at DESC);

-- Each run's outcome and coverage per source.
CREATE TABLE ai_provider_import_run_sources
(
    run_id           BIGINT  NOT NULL REFERENCES ai_provider_import_runs (id) ON DELETE CASCADE,
    source           TEXT    NOT NULL,
    status           TEXT    NOT NULL,
    requests         INTEGER NOT NULL,
    slices_fetched   INTEGER NOT NULL,
    slices_changed   INTEGER NOT NULL,
    -- Every expected slice through this UTC day was fetched after its day ended.
    imported_through DATE,
    -- The longest late change seen in the last 62 days, in days; NULL while none was seen.
    settle_lag_days  INTEGER,
    error            TEXT,
    CONSTRAINT ai_provider_import_run_sources_pkey PRIMARY KEY (run_id, source),
    CONSTRAINT ai_provider_import_run_sources_source_check
        CHECK (source IN ('account_credits', 'org_credits', 'org_usage', 'summary')),
    CONSTRAINT ai_provider_import_run_sources_status_check
        CHECK (status IN ('succeeded', 'partial', 'failed', 'skipped')),
    CONSTRAINT ai_provider_import_run_sources_counts_check CHECK (
        requests >= 0 AND slices_fetched >= 0 AND slices_changed >= 0
            AND (settle_lag_days IS NULL OR settle_lag_days >= 0)),
    CONSTRAINT ai_provider_import_run_sources_error_check CHECK (char_length(error) BETWEEN 1 AND 2000)
);
