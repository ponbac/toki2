-- When the newest applied upload from a machine started reading its history,
-- capped at the time the server stored it. Uploads replace whole windows, so an
-- upload that read history earlier must not overwrite one that read it later.
-- NULL until a client that reports the instant uploads.
ALTER TABLE ai_usage_machines ADD COLUMN last_snapshot_at TIMESTAMPTZ;
