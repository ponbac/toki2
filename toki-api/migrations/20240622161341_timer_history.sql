-- Milltime Timer History
CREATE TABLE timer_history
(
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    project_id TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
