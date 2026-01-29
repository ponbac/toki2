-- Index for active timer lookups (most frequent query pattern)
CREATE INDEX idx_timer_history_user_active
    ON timer_history(user_id)
    WHERE end_time IS NULL;

-- Index for registration_id lookups
CREATE INDEX idx_timer_history_registration_id
    ON timer_history(registration_id)
    WHERE registration_id IS NOT NULL;

-- Index for fetching all timer history for a user
CREATE INDEX idx_timer_history_user_id
    ON timer_history(user_id);
