-- Accounts at metered AI payers, their seat history, and whose usage each
-- account's is (dispositions). For Copilot an account is a GitHub user,
-- identified by its numeric id, which survives renames; login is the latest
-- seen. Toki reads seats and public profiles from the payer's account
-- directory (a GitHub App on the organization) daily and when an admin asks.
-- Nothing is ever deleted: a seat the directory stops listing is ended and its
-- account kept, so credits on ended seats always have an account. Accounts
-- seen only in imported usage are added by the usage import. btree_gist, which
-- the exclusion constraints need, is enabled by
-- 20260928140000_add_ai_subscriptions.sql.

CREATE TABLE ai_provider_accounts
(
    provider           TEXT        NOT NULL,
    account_id         BIGINT      NOT NULL,
    login              TEXT        NOT NULL,
    -- The public profile as last read; NULL where the account shows none.
    profile_name       TEXT,
    profile_email      TEXT,
    profile_fetched_at TIMESTAMPTZ,
    first_seen_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ai_provider_accounts_pkey PRIMARY KEY (provider, account_id),
    CONSTRAINT ai_provider_accounts_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_accounts_account_id_check CHECK (account_id > 0),
    CONSTRAINT ai_provider_accounts_login_check
        CHECK (char_length(login) BETWEEN 1 AND 100 AND login !~ '[[:space:][:cntrl:]]'),
    CONSTRAINT ai_provider_accounts_profile_name_check
        CHECK (char_length(profile_name) BETWEEN 1 AND 512),
    CONSTRAINT ai_provider_accounts_profile_email_check
        CHECK (char_length(profile_email) BETWEEN 1 AND 512),
    CONSTRAINT ai_provider_accounts_profile_fetched_check
        CHECK (profile_fetched_at IS NOT NULL OR (profile_name IS NULL AND profile_email IS NULL))
);

-- One row per seat assignment the directory listed. seat_created_at is the
-- payer's own assignment instant (compared as an instant, whatever offset the
-- payer renders it in), so a seat removed and assigned again is a new row. An
-- account holds at most one open (still listed) seat.
CREATE TABLE ai_provider_seats
(
    id                      BIGSERIAL   PRIMARY KEY,
    provider                TEXT        NOT NULL,
    account_id              BIGINT      NOT NULL,
    seat_created_at         TIMESTAMPTZ NOT NULL,
    -- The day the payer will cancel the seat; NULL unless a cancellation is pending.
    pending_cancellation_on DATE,
    last_activity_at        TIMESTAMPTZ,
    -- The client of the last activity, such as vscode/1.143.0/copilot-chat/0.71.0.
    last_activity_client    TEXT,
    first_listed_at         TIMESTAMPTZ NOT NULL,
    last_listed_at          TIMESTAMPTZ NOT NULL,
    -- The first complete listing without the seat; NULL while it is listed.
    unlisted_at             TIMESTAMPTZ,
    CONSTRAINT ai_provider_seats_account_fkey FOREIGN KEY (provider, account_id)
        REFERENCES ai_provider_accounts (provider, account_id),
    CONSTRAINT ai_provider_seats_assignment_key UNIQUE (provider, account_id, seat_created_at),
    CONSTRAINT ai_provider_seats_listed_check CHECK (
        first_listed_at <= last_listed_at
            AND (unlisted_at IS NULL OR last_listed_at < unlisted_at)
        ),
    CONSTRAINT ai_provider_seats_client_check
        CHECK (char_length(last_activity_client) BETWEEN 1 AND 512)
);

CREATE UNIQUE INDEX ai_provider_seats_open_idx
    ON ai_provider_seats (provider, account_id) WHERE unlisted_at IS NULL;

-- Each payer directory's last refresh attempt and last success, so the daily
-- refresh survives restarts and the admin page can say how fresh seats are.
CREATE TABLE ai_provider_directory_status
(
    provider        TEXT        PRIMARY KEY,
    last_attempt_at TIMESTAMPTZ NOT NULL,
    last_success_at TIMESTAMPTZ,
    -- Why the last attempt failed; NULL when it succeeded.
    last_error      TEXT,
    -- What the last successful listing held and what the payer reported.
    seats_listed    INTEGER,
    seats_reported  INTEGER,
    CONSTRAINT ai_provider_directory_status_provider_check CHECK (provider IN ('copilot')),
    CONSTRAINT ai_provider_directory_status_error_check
        CHECK (char_length(last_error) BETWEEN 1 AND 2000),
    CONSTRAINT ai_provider_directory_status_success_check
        CHECK (last_success_at IS NULL OR last_success_at <= last_attempt_at),
    CONSTRAINT ai_provider_directory_status_counts_check
        CHECK (seats_listed >= 0 AND seats_reported >= 0)
);

-- Whose an account's usage is on each UTC day from valid_from through valid_to
-- (both inclusive; NULL while ongoing): a Toki user's ('user'), or no Toki
-- user's, so Spinit overhead ('external'). An account's dispositions never
-- share a day, and neither do one user's for one payer. Externals never
-- conflict with each other (user_id is NULL). An account without a disposition
-- for a day is unlinked that day. login is the account's login when the row
-- was last written.
CREATE TABLE ai_provider_account_dispositions
(
    id         SERIAL      PRIMARY KEY,
    provider   TEXT        NOT NULL,
    account_id BIGINT      NOT NULL,
    login      TEXT        NOT NULL,
    target     TEXT        NOT NULL,
    user_id    INTEGER     REFERENCES users (id) ON DELETE CASCADE,
    valid_from DATE        NOT NULL,
    valid_to   DATE,
    created_by INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by INTEGER     REFERENCES users (id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ai_provider_account_dispositions_account_fkey FOREIGN KEY (provider, account_id)
        REFERENCES ai_provider_accounts (provider, account_id),
    CONSTRAINT ai_provider_account_dispositions_target_check CHECK (
        (target = 'user' AND user_id IS NOT NULL) OR (target = 'external' AND user_id IS NULL)
        ),
    CONSTRAINT ai_provider_account_dispositions_login_check
        CHECK (char_length(login) BETWEEN 1 AND 100),
    CONSTRAINT ai_provider_account_dispositions_period_check CHECK (
        valid_from BETWEEN DATE '0001-01-01' AND DATE '9999-12-31'
            AND (valid_to IS NULL OR (valid_from <= valid_to AND valid_to <= DATE '9999-12-31'))
        ),
    CONSTRAINT ai_provider_account_dispositions_account_no_overlap EXCLUDE USING gist (
        provider WITH =, account_id WITH =, daterange(valid_from, valid_to, '[]') WITH &&
        ),
    CONSTRAINT ai_provider_account_dispositions_user_no_overlap EXCLUDE USING gist (
        provider WITH =, user_id WITH =, daterange(valid_from, valid_to, '[]') WITH &&
        )
);
