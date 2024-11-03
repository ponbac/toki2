-- Create notification_type enum
CREATE TYPE notification_type AS ENUM
(
    'pr_closed',
    'thread_added',
    'thread_updated'
);

-- Create notification rules table (per repository and notification type settings)
CREATE TABLE notification_rules
(
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL,
    repository_id INT NOT NULL,
    notification_type notification_type NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    UNIQUE (user_id, repository_id, notification_type)
);

-- Create PR-specific notification exceptions
CREATE TABLE pr_notification_exceptions
(
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL,
    repository_id INT NOT NULL,
    pull_request_id INT NOT NULL,
    notification_type notification_type NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
    UNIQUE (user_id, repository_id, pull_request_id, notification_type)
);

-- Create notifications table
CREATE TABLE notifications
(
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL,
    repository_id INT NOT NULL,
    pull_request_id INT NOT NULL,
    notification_type notification_type NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    link TEXT,
    viewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'
    ::jsonb,
    FOREIGN KEY
    (user_id) REFERENCES users
    (id) ON
    DELETE CASCADE,
    FOREIGN KEY (repository_id)
    REFERENCES repositories
    (id) ON
    DELETE CASCADE
);

    -- Indexes for faster queries
    CREATE INDEX idx_notifications_user_viewed 
ON notifications(user_id, viewed_at);

    CREATE INDEX idx_notifications_repo
ON notifications(repository_id, user_id);

    CREATE INDEX idx_notification_rules_lookup
ON notification_rules(user_id, repository_id);

    CREATE INDEX idx_pr_exceptions_lookup
ON pr_notification_exceptions(user_id, repository_id, pull_request_id);
