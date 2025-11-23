ALTER TABLE user_avatars
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

UPDATE user_avatars
SET updated_at = COALESCE(updated_at, created_at, now());


