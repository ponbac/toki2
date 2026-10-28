-- Before importing a month, a run marks every check of that month with its id,
-- and each scope's new check clears the mark. A check still marked by a run
-- that is no longer importing is interrupted: the run stopped (a rate limit,
-- the request cap, ...) or its process died before finishing that scope's
-- month, which may have left its days half replaced, or older than the
-- organization's refreshed days. Such a check no longer vouches for the stored
-- days, whatever the account's seat, and its month is imported again.
ALTER TABLE ai_provider_usage_checks
    ADD COLUMN importing_run_id BIGINT REFERENCES ai_provider_import_runs (id);
