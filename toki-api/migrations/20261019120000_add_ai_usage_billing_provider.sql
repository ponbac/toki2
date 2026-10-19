-- Keep the source tool separate from an explicitly observed subscription payer.
ALTER TABLE ai_usage_buckets ADD COLUMN billing_provider TEXT;
ALTER TABLE ai_usage_buckets ADD CONSTRAINT ai_usage_buckets_billing_provider_check
    CHECK (billing_provider IN ('codex', 'claude', 'grok', 'copilot')
        AND billing_provider <> provider);

DROP INDEX ai_usage_buckets_key_idx;
CREATE UNIQUE INDEX ai_usage_buckets_key_idx ON ai_usage_buckets (
    machine_id, hour_start, provider, session_key, md5(project_key), md5(model),
    COALESCE(billing_provider, '')
);
