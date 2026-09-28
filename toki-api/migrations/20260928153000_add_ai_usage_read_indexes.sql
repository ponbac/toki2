-- Indexes for reading one user's AI usage back.
--
-- A user's project keys and when each was last used: reads skip from key to
-- key and take each key's latest hour, instead of scanning the user's whole
-- history. One 512-character key (at most 2048 bytes), a user id and an
-- instant fit a B-tree entry.
CREATE INDEX ai_usage_buckets_user_id_project_key_hour_start_idx
    ON ai_usage_buckets (user_id, project_key, hour_start);

-- Whether a machine has stored usage of a provider, which the bucket key index
-- cannot answer without scanning the machine's history: provider is its third
-- column.
CREATE INDEX ai_usage_buckets_machine_id_provider_idx ON ai_usage_buckets (machine_id, provider);
