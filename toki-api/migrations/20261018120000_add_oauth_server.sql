-- Toki's OAuth 2.1 authorization server, through which MCP clients connect to
-- /mcp on a user's behalf. Codes, tokens and consent handles are stored only as
-- SHA-256 hashes.

-- Clients that registered dynamically (RFC 7591). Clients identified by a
-- Client ID Metadata Document are fetched, not stored.
CREATE TABLE oauth_clients
(
    client_id     TEXT        PRIMARY KEY,
    client_name   TEXT        NOT NULL,
    redirect_uris TEXT[]      NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- When the client last obtained a grant. Registrations that never did are
    -- deleted after 30 days; others are kept, as clients cache their client_id.
    last_used_at  TIMESTAMPTZ,
    CONSTRAINT oauth_clients_redirect_uris_check
        CHECK (cardinality(redirect_uris) BETWEEN 1 AND 10)
);

-- Consent pages awaiting the user's decision, each decided at most once.
CREATE TABLE oauth_authorization_requests
(
    id_hash        BYTEA       PRIMARY KEY,
    user_id        INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id      TEXT        NOT NULL,
    client_name    TEXT        NOT NULL,
    redirect_uri   TEXT        NOT NULL,
    resource       TEXT        NOT NULL,
    code_challenge TEXT        NOT NULL,
    state          TEXT,
    expires_at     TIMESTAMPTZ NOT NULL,
    CONSTRAINT oauth_authorization_requests_id_hash_length_check
        CHECK (octet_length(id_hash) = 32)
);

-- An approved connection: one client's access as one user, and the family of
-- refresh tokens rotated from it.
CREATE TABLE oauth_grants
(
    id           SERIAL      PRIMARY KEY,
    user_id      INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id    TEXT        NOT NULL,
    client_name  TEXT        NOT NULL,
    redirect_uri TEXT        NOT NULL,
    resource     TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ
);

CREATE INDEX oauth_grants_user_id_idx ON oauth_grants (user_id);

CREATE TABLE oauth_authorization_codes
(
    code_hash      BYTEA       PRIMARY KEY,
    user_id        INTEGER     NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id      TEXT        NOT NULL,
    client_name    TEXT        NOT NULL,
    redirect_uri   TEXT        NOT NULL,
    resource       TEXT        NOT NULL,
    code_challenge TEXT        NOT NULL,
    expires_at     TIMESTAMPTZ NOT NULL,
    consumed_at    TIMESTAMPTZ,
    -- The grant the code produced, revoked if the code is presented again.
    grant_id       INTEGER     REFERENCES oauth_grants (id) ON DELETE CASCADE,
    CONSTRAINT oauth_authorization_codes_code_hash_length_check
        CHECK (octet_length(code_hash) = 32)
);

CREATE INDEX oauth_authorization_codes_grant_id_idx ON oauth_authorization_codes (grant_id);

CREATE TABLE oauth_tokens
(
    token_hash BYTEA       PRIMARY KEY,
    grant_id   INTEGER     NOT NULL REFERENCES oauth_grants (id) ON DELETE CASCADE,
    kind       TEXT        NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    -- When a refresh token was exchanged for its successor.
    rotated_at TIMESTAMPTZ,
    CONSTRAINT oauth_tokens_token_hash_length_check
        CHECK (octet_length(token_hash) = 32),
    CONSTRAINT oauth_tokens_kind_check
        CHECK (kind IN ('access', 'refresh')),
    CONSTRAINT oauth_tokens_rotation_check
        CHECK (kind = 'refresh' OR rotated_at IS NULL)
);

CREATE INDEX oauth_tokens_grant_id_idx ON oauth_tokens (grant_id);
