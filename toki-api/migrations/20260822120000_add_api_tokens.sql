-- Personal access tokens for non-browser clients (TUI, desktop widgets, curl).
-- Only the SHA-256 hash is stored; the plaintext secret is shown once at creation.

CREATE TABLE api_tokens
(
    id            SERIAL PRIMARY KEY,
    user_id       INTEGER      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name          TEXT         NOT NULL,
    token_prefix  TEXT         NOT NULL,
    token_hash    BYTEA        NOT NULL,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    CONSTRAINT api_tokens_token_hash_key UNIQUE (token_hash),
    CONSTRAINT api_tokens_name_length_check
        CHECK (char_length(name) BETWEEN 1 AND 64),
    CONSTRAINT api_tokens_name_characters_check
        CHECK (name !~ '[[:cntrl:]]'),
    CONSTRAINT api_tokens_prefix_format_check
        CHECK (token_prefix ~ '^toki_[0-9a-f]{7}$'),
    CONSTRAINT api_tokens_hash_length_check
        CHECK (octet_length(token_hash) = 32)
);

CREATE INDEX api_tokens_user_id_idx ON api_tokens (user_id);
