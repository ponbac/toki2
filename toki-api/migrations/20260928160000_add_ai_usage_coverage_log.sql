-- A log of what each sync proved: for every provider an upload replaced (coverage
-- `ok` or `partial`), the machine's stored usage of that provider is complete from
-- the window start until the window end or the report time, whichever is earlier.
-- Usage recorded after an upload cannot be in it. Billing checks that the union of
-- these intervals spans every day of a month before relying on it. Each sync adds
-- rows; a repeated upload only adds an interval already covered.

CREATE TABLE ai_usage_coverage_log
(
    id            BIGSERIAL   PRIMARY KEY,
    machine_id    UUID        NOT NULL REFERENCES ai_usage_machines (id) ON DELETE CASCADE,
    provider      TEXT        NOT NULL,
    window_start  TIMESTAMPTZ NOT NULL,
    -- The window end, capped at `reported_at`.
    covered_until TIMESTAMPTZ NOT NULL,
    reported_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ai_usage_coverage_log_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot')),
    CONSTRAINT ai_usage_coverage_log_interval_check
        CHECK (window_start < covered_until AND covered_until <= reported_at)
);

CREATE INDEX ai_usage_coverage_log_machine_id_covered_until_idx
    ON ai_usage_coverage_log (machine_id, covered_until);

-- Usage stored before this log existed: the latest report of each provider is the
-- only sync still known. Earlier syncs are lost, so their days stay unproven.
INSERT INTO ai_usage_coverage_log (machine_id, provider, window_start, covered_until, reported_at)
SELECT machine_id, provider, window_start, least(window_end, reported_at), reported_at
FROM ai_usage_machine_coverage
WHERE status IN ('ok', 'partial')
  AND window_start < least(window_end, reported_at);

-- Billing reads everyone's usage for a month by hour alone.
CREATE INDEX ai_usage_buckets_hour_start_idx ON ai_usage_buckets (hour_start);
