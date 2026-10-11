-- Exchange rates that convert AI billing to the billing currency (SEK), one row per
-- billing month and foreign currency, stored so that a month's bill can be
-- reproduced.
--
-- Rates are how many units of the billing currency one unit of `currency` is worth.
-- A row keeps two rates apart:
--
-- - The fetched rate: the exchange-rate provider's average of the month's daily
--   rates. `fetched_source` names the provider, such as `riksbank`. It is
--   `fetched_provisional` while it is an average of the days published so far
--   (`fetched_through` is the latest day it includes), and is replaced by the final
--   monthly average once the month is over.
-- - An admin's override. It takes precedence over the fetched rate and a fetch never
--   touches it. Resetting clears it, and the fetched rate applies again.
CREATE TABLE ai_billing_exchange_rates
(
    -- The first day of the billing month.
    month               DATE           NOT NULL,
    currency            TEXT           NOT NULL,
    fetched_rate        NUMERIC(18, 6),
    fetched_source      TEXT,
    fetched_provisional BOOLEAN,
    fetched_through     DATE,
    fetched_at          TIMESTAMPTZ,
    override_rate       NUMERIC(18, 6),
    overridden_at       TIMESTAMPTZ,
    -- The admin who set the override; NULL once they are gone.
    overridden_by       INTEGER        REFERENCES users (id) ON DELETE SET NULL,
    PRIMARY KEY (month, currency),
    CONSTRAINT ai_billing_exchange_rates_month_check
        CHECK (isfinite(month) AND EXTRACT(DAY FROM month) = 1),
    -- An ISO 4217 code, such as USD or EUR.
    CONSTRAINT ai_billing_exchange_rates_currency_check CHECK (currency ~ '^[A-Z]{3}$'),
    -- A missing rate is a missing value, never zero.
    CONSTRAINT ai_billing_exchange_rates_rates_check
        CHECK (fetched_rate > 0 AND override_rate > 0),
    CONSTRAINT ai_billing_exchange_rates_fetched_check CHECK (
        (fetched_rate IS NULL) = (fetched_source IS NULL)
            AND (fetched_rate IS NULL) = (fetched_provisional IS NULL)
            AND (fetched_rate IS NULL) = (fetched_through IS NULL)
            AND (fetched_rate IS NULL) = (fetched_at IS NULL)
        ),
    CONSTRAINT ai_billing_exchange_rates_fetched_source_check
        CHECK (char_length(fetched_source) BETWEEN 1 AND 64 AND fetched_source !~ '[[:cntrl:]]'),
    CONSTRAINT ai_billing_exchange_rates_override_check
        CHECK ((override_rate IS NULL) = (overridden_at IS NULL)
            AND (override_rate IS NOT NULL OR overridden_by IS NULL)),
    -- A row holds a rate.
    CONSTRAINT ai_billing_exchange_rates_any_rate_check
        CHECK (fetched_rate IS NOT NULL OR override_rate IS NOT NULL)
);
