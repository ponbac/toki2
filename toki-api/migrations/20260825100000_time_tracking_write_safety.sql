-- Keep the newest active row per user, matching the previous lazy cleanup rule.
WITH ranked_active_timers AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY user_id
               ORDER BY created_at DESC, id DESC
           ) AS position
    FROM timer_history
    WHERE end_time IS NULL
)
DELETE FROM timer_history
WHERE id IN (
    SELECT id FROM ranked_active_timers WHERE position > 1
);

DROP INDEX IF EXISTS idx_timer_history_user_active;

CREATE UNIQUE INDEX idx_timer_history_one_active_per_user
    ON timer_history(user_id)
    WHERE end_time IS NULL;

CREATE TABLE time_tracking_idempotency
(
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    provider_operation_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'completed')),
    result JSONB,
    locked_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, operation, idempotency_key),
    CHECK (char_length(idempotency_key) BETWEEN 1 AND 200),
    CHECK ((state = 'completed' AND result IS NOT NULL) OR state = 'pending')
);

CREATE INDEX idx_time_tracking_idempotency_created_at
    ON time_tracking_idempotency(created_at);
