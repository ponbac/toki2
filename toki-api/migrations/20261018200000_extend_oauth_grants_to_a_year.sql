-- Connections now last at most a year instead of 90 days. Give the live ones the
-- same lifetime from when they were made, so they do not lapse first.
UPDATE oauth_grants
SET expires_at = created_at + INTERVAL '365 days'
WHERE revoked_at IS NULL
  AND expires_at > now();
