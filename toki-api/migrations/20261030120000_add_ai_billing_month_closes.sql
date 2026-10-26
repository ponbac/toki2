-- Month close: a month's whole AI bill (API lines, subscription fee shares and overhead, metered
-- Copilot lines) frozen as it was invoiced. A close stores one ai_billing_month_closes row and one
-- ai_billing_snapshot_lines row per line as its readers saw it (one per Unassigned kind): the
-- month's own lines, and the adjustment lines posted on it to earlier closed months
-- (adjusts_month). A closed month is served from its snapshot, JSON and CSV. A later change to a
-- closed month N posts on the first open month after N, per diff key (subject, project, kind,
-- currency, provider or subscription): recompute(N) - snapshot(N) - the adjustments to N frozen in
-- active closes; the SEK amount is the SEK difference at N's rate. A re-open keeps its close as
-- history (reopened_at); computations read active closes only. A month can be re-opened only
-- while no active close holds adjustments to it.

CREATE TABLE ai_billing_month_closes
(
    id                 BIGSERIAL   PRIMARY KEY,
    -- The closed month's first day.
    month              DATE        NOT NULL,
    -- What the close freezes: 'ai_bill', the whole AI bill.
    scope              TEXT        NOT NULL DEFAULT 'ai_bill',
    closed_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The admin who closed it; NULL once that user is deleted.
    closed_by          INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    -- The evaluated readiness checks, with their problems and each waived check's reason.
    readiness_json     JSONB       NOT NULL,
    -- The overview as served at close without lines, adjustmentLines and close.
    bill_json          JSONB       NOT NULL,
    -- Per user id, the developer page's bill part: subscriptions, totals, metered.
    developer_json     JSONB       NOT NULL,
    -- The header the snapshot's csv_row cells follow.
    csv_header         TEXT[]      NOT NULL,
    -- The shape of the stored JSON; a change to the served JSON bumps it with an upgrade step.
    projection_version SMALLINT    NOT NULL,
    reopened_at        TIMESTAMPTZ,
    reopened_by        INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT ai_billing_month_closes_month_check
        CHECK (isfinite(month) AND EXTRACT(DAY FROM month) = 1),
    CONSTRAINT ai_billing_month_closes_scope_check CHECK (scope IN ('ai_bill')),
    CONSTRAINT ai_billing_month_closes_json_check CHECK (
        jsonb_typeof(readiness_json) = 'object' AND jsonb_typeof(bill_json) = 'object'
            AND jsonb_typeof(developer_json) = 'object'),
    CONSTRAINT ai_billing_month_closes_header_check CHECK (cardinality(csv_header) > 0),
    CONSTRAINT ai_billing_month_closes_version_check CHECK (projection_version >= 1),
    CONSTRAINT ai_billing_month_closes_reopened_check CHECK (
        (reopened_at IS NOT NULL OR reopened_by IS NULL)
            AND (reopened_at IS NULL OR reopened_at >= closed_at))
);

-- At most one active close per scope and month.
CREATE UNIQUE INDEX ai_billing_month_closes_active_key
    ON ai_billing_month_closes (scope, month) WHERE reopened_at IS NULL;

CREATE TABLE ai_billing_snapshot_lines
(
    close_id         BIGINT         NOT NULL REFERENCES ai_billing_month_closes (id) ON DELETE CASCADE,
    -- Own lines first, then adjustment lines, each in served order.
    position         INTEGER        NOT NULL,
    -- The row's place in the CSV.
    csv_position     INTEGER        NOT NULL,
    -- An adjustment line's corrected month (its first day) and that month's close; NULL on own lines.
    adjusts_month    DATE,
    adjusts_close_id BIGINT         REFERENCES ai_billing_month_closes (id),
    -- The diff key, as text: subject, project, kind, currency, and the payer: subscription_id
    -- for subscription and overhead lines, else provider.
    subject          TEXT           NOT NULL,
    -- An account's login or a repository's spelling; NULL for users and the unknown account.
    subject_label    TEXT,
    project          TEXT           NOT NULL,
    project_name     TEXT,
    kind             TEXT           NOT NULL,
    currency         TEXT           NOT NULL,
    -- The line's provider as shown (a summed subscription key: the subscription's provider).
    -- Mirrors AiProvider, as ai_usage_buckets does: a new provider extends this check too.
    provider         TEXT           NOT NULL,
    -- Subscription and overhead lines. No foreign key: a subscription can be deleted after close.
    subscription_id  INTEGER,
    -- Hundredths of `currency`; NULL when an API line's cost is unknown. Negative only on
    -- adjustments.
    amount           BIGINT,
    -- Hundredths of the billing currency; NULL without a rate or a known amount.
    sek_amount       BIGINT,
    -- The rate sek_amount used (an adjustment: the corrected month's); NULL in the billing currency.
    rate             NUMERIC(18, 6),
    -- Metered kinds' credits (a credit is a cent of USD).
    credits          BIGINT,
    line_json        JSONB          NOT NULL,
    -- Raw cell values, guarded when served.
    csv_row          TEXT[]         NOT NULL,
    CONSTRAINT ai_billing_snapshot_lines_pkey PRIMARY KEY (close_id, position),
    CONSTRAINT ai_billing_snapshot_lines_csv_key UNIQUE (close_id, csv_position),
    CONSTRAINT ai_billing_snapshot_lines_positions_check
        CHECK (position >= 0 AND csv_position >= 0),
    CONSTRAINT ai_billing_snapshot_lines_adjusts_check CHECK (
        (adjusts_month IS NULL) = (adjusts_close_id IS NULL)
            AND (adjusts_month IS NULL
                OR (isfinite(adjusts_month) AND EXTRACT(DAY FROM adjusts_month) = 1))),
    CONSTRAINT ai_billing_snapshot_lines_subject_check CHECK (
        subject ~ '^(user:[0-9]+|account:[a-z]+:[0-9]+|unknown:[a-z]+|repository:[a-z]+:.+)$'
            AND (subject_label IS NOT NULL) = (subject ~ '^(account|repository):')),
    CONSTRAINT ai_billing_snapshot_lines_project_check CHECK (
        project ~ '^(project:.+|unassigned:(unmapped|unmapped_repository|unattributed)|none)$'
            AND (project_name IS NOT NULL) = (project LIKE 'project:%')),
    CONSTRAINT ai_billing_snapshot_lines_kind_check CHECK (kind IN (
        'api', 'subscription', 'overhead', 'metered_project', 'metered_personal',
        'metered_untracked', 'metered_unlinked_account', 'metered_external_account',
        'metered_unknown_account', 'metered_repository')),
    CONSTRAINT ai_billing_snapshot_lines_currency_check CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT ai_billing_snapshot_lines_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode')),
    CONSTRAINT ai_billing_snapshot_lines_subscription_check
        CHECK ((subscription_id IS NOT NULL) = (kind IN ('subscription', 'overhead'))),
    CONSTRAINT ai_billing_snapshot_lines_credits_check
        CHECK ((credits IS NOT NULL) = (kind LIKE 'metered%')),
    CONSTRAINT ai_billing_snapshot_lines_amount_check
        CHECK (amount IS NOT NULL OR (kind = 'api' AND adjusts_month IS NULL)),
    CONSTRAINT ai_billing_snapshot_lines_sign_check CHECK (
        adjusts_month IS NOT NULL
            OR (coalesce(amount, 0) >= 0 AND coalesce(sek_amount, 0) >= 0
                AND coalesce(credits, 0) >= 0)),
    CONSTRAINT ai_billing_snapshot_lines_rate_check CHECK (rate > 0),
    CONSTRAINT ai_billing_snapshot_lines_payload_check
        CHECK (jsonb_typeof(line_json) = 'object' AND cardinality(csv_row) > 0)
);

-- Adjustments to a month frozen in closes: netting and the re-open check.
CREATE INDEX ai_billing_snapshot_lines_adjusts_idx
    ON ai_billing_snapshot_lines (adjusts_month, close_id) WHERE adjusts_month IS NOT NULL;
