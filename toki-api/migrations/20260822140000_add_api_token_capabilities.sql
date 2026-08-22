-- Durable per-token capabilities. Existing tokens stay least-privileged.
ALTER TABLE api_tokens
    ADD COLUMN capabilities TEXT[] NOT NULL DEFAULT ARRAY['timer:read'];

ALTER TABLE api_tokens
    ADD CONSTRAINT api_tokens_capabilities_not_empty
        CHECK (cardinality(capabilities) >= 1);

ALTER TABLE api_tokens
    ADD CONSTRAINT api_tokens_capabilities_known
        CHECK (
            capabilities <@ ARRAY[
                'timer:read',
                'catalog:read',
                'entries:read',
                'work-items:read',
                'pull-requests:read'
            ]::text[]
        );
