CREATE TABLE IF NOT EXISTS user_avatars
(
    user_id    INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    image      BYTEA       NOT NULL,
    mime_type  TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
