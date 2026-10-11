-- AI subscriptions that developers declare, such as a Claude Max or ChatGPT Pro plan.
-- A provider's usage on a local day that none of the user's subscriptions to that
-- provider covers is billed as API usage at its estimated cost.
-- Dates are local calendar dates in the `ai_usage.time_zone` setting. `valid_to` is
-- inclusive, and NULL while the subscription is ongoing. The monthly fee is exact and
-- kept in the subscription's own currency.

-- Lets the exclusion constraint below compare user_id and provider with `=`.
-- btree_gist ships with PostgreSQL and is a trusted extension since version 13.
CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE ai_subscriptions
(
    id           SERIAL PRIMARY KEY,
    user_id      INTEGER        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider     TEXT           NOT NULL,
    plan         TEXT           NOT NULL,
    monthly_cost NUMERIC(12, 2) NOT NULL,
    currency     TEXT           NOT NULL,
    valid_from   DATE           NOT NULL,
    valid_to     DATE,
    created_at   TIMESTAMPTZ    NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ    NOT NULL DEFAULT now(),
    CONSTRAINT ai_subscriptions_provider_check
        CHECK (provider IN ('codex', 'claude', 'grok', 'copilot')),
    CONSTRAINT ai_subscriptions_plan_check
        CHECK (char_length(plan) BETWEEN 1 AND 64 AND plan !~ '[[:cntrl:]]'),
    CONSTRAINT ai_subscriptions_monthly_cost_check CHECK (monthly_cost >= 0),
    -- An ISO 4217 code, such as SEK or USD.
    CONSTRAINT ai_subscriptions_currency_check CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT ai_subscriptions_period_check CHECK (
        valid_from BETWEEN DATE '0001-01-01' AND DATE '9999-12-31'
            AND (valid_to IS NULL OR (
                valid_from <= valid_to AND valid_to <= DATE '9999-12-31'
            ))
        ),
    -- One user's subscriptions to one provider never share a day, so at most one
    -- subscription decides how a day's usage is billed. An ongoing subscription
    -- covers every day from valid_from on.
    CONSTRAINT ai_subscriptions_no_overlap EXCLUDE USING gist (
        user_id WITH =,
        provider WITH =,
        daterange(valid_from, valid_to, '[]') WITH &&
        )
);
