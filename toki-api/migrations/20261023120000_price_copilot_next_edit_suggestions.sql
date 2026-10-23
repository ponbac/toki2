-- GitHub bills no Copilot plan for next edit suggestions, so their usage costs
-- nothing. token-ledger finds no API price for these models and uploaded them as
-- unpriced; uploads are now stored at zero cost, and this gives stored usage the
-- same cost. Usage another subscription paid for keeps its uploaded cost.
-- The prefixes match `is_next_edit_suggestions_model` in the domain.
UPDATE ai_usage_buckets
SET estimated_cost_usd = 0,
    unpriced_records   = 0
WHERE provider = 'copilot'
  AND billing_provider IS NULL
  AND (starts_with(model, 'copilot-nes-') OR starts_with(model, 'nes-'))
  AND (estimated_cost_usd IS DISTINCT FROM 0 OR unpriced_records <> 0);
