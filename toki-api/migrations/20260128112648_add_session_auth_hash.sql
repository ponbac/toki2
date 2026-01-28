-- Add session_auth_hash column for stable session validation across logins
-- This allows multi-device sessions by not invalidating sessions on token refresh

ALTER TABLE users
ADD COLUMN session_auth_hash TEXT NOT NULL DEFAULT gen_random_uuid()::TEXT;
