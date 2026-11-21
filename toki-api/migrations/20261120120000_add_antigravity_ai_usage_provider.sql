-- Antigravity (the `antigravity` coding agent) becomes a provider whose usage
-- token-ledger uploads and for which developers can declare subscriptions. Each
-- constraint is replaced by a wider one in a single statement, so no row is ever
-- unchecked and every stored row already satisfies it.

ALTER TABLE ai_usage_machine_coverage
    DROP CONSTRAINT ai_usage_machine_coverage_provider_check,
    ADD CONSTRAINT ai_usage_machine_coverage_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode', 'antigravity'));

ALTER TABLE ai_usage_buckets
    DROP CONSTRAINT ai_usage_buckets_provider_check,
    ADD CONSTRAINT ai_usage_buckets_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode', 'antigravity')),
    DROP CONSTRAINT ai_usage_buckets_billing_provider_check,
    ADD CONSTRAINT ai_usage_buckets_billing_provider_check
        CHECK (billing_provider IN ('codex', 'claude', 'grok', 'copilot', 'antigravity')
            AND billing_provider <> provider);

ALTER TABLE ai_usage_provider_hints
    DROP CONSTRAINT ai_usage_provider_hints_provider_check,
    ADD CONSTRAINT ai_usage_provider_hints_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode', 'antigravity'));

ALTER TABLE ai_usage_coverage_log
    DROP CONSTRAINT ai_usage_coverage_log_provider_check,
    ADD CONSTRAINT ai_usage_coverage_log_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode', 'antigravity'));

ALTER TABLE ai_subscriptions
    DROP CONSTRAINT ai_subscriptions_provider_check,
    ADD CONSTRAINT ai_subscriptions_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'antigravity'));

ALTER TABLE ai_billing_snapshot_lines
    DROP CONSTRAINT ai_billing_snapshot_lines_provider_check,
    ADD CONSTRAINT ai_billing_snapshot_lines_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi', 'opencode', 'antigravity'));
