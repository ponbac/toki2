-- Pi (the `pi` coding agent) becomes a provider whose usage token-ledger uploads.
-- Each constraint is replaced by a wider one in a single statement, so no row is
-- ever unchecked and every stored row already satisfies it.
--
-- `ai_subscriptions` keeps its providers: Pi has no subscriptions of its own. A
-- ChatGPT subscription, declared for `codex`, pays for the Pi usage of a local day
-- on which Pi reported its ChatGPT sign-in as a plan hint.

ALTER TABLE ai_usage_machine_coverage
    DROP CONSTRAINT ai_usage_machine_coverage_provider_check,
    ADD CONSTRAINT ai_usage_machine_coverage_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi'));

ALTER TABLE ai_usage_buckets
    DROP CONSTRAINT ai_usage_buckets_provider_check,
    ADD CONSTRAINT ai_usage_buckets_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi'));

ALTER TABLE ai_usage_provider_hints
    DROP CONSTRAINT ai_usage_provider_hints_provider_check,
    ADD CONSTRAINT ai_usage_provider_hints_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi'));

ALTER TABLE ai_usage_coverage_log
    DROP CONSTRAINT ai_usage_coverage_log_provider_check,
    ADD CONSTRAINT ai_usage_coverage_log_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot', 'pi'));
