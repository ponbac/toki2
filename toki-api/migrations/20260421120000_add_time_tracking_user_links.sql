CREATE TABLE time_tracking_provider_users
(
    id SERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_company_id TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    foreign_id TEXT,
    internal_id TEXT,
    name TEXT NOT NULL,
    email TEXT,
    active BOOLEAN NOT NULL,
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (provider, provider_company_id, provider_user_id)
);

CREATE INDEX idx_time_tracking_provider_users_email
    ON time_tracking_provider_users(provider, provider_company_id, lower(email))
    WHERE email IS NOT NULL;

CREATE TABLE time_tracking_user_links
(
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_company_id TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    provider_user_email TEXT,
    provider_user_name TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_synced_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (provider, provider_company_id, provider_user_id)
        REFERENCES time_tracking_provider_users(provider, provider_company_id, provider_user_id)
);

CREATE UNIQUE INDEX uq_time_tracking_user_links_active_user
    ON time_tracking_user_links(user_id, provider)
    WHERE active;

CREATE UNIQUE INDEX uq_time_tracking_user_links_active_provider_user
    ON time_tracking_user_links(provider, provider_company_id, provider_user_id)
    WHERE active;

CREATE INDEX idx_time_tracking_user_links_provider_user
    ON time_tracking_user_links(provider, provider_company_id, provider_user_id);
