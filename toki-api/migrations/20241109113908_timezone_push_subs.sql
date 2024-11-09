-- Convert created_at column to TIMESTAMPTZ and make it NOT NULL
ALTER TABLE push_subscriptions
    ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN created_at SET NOT NULL;
