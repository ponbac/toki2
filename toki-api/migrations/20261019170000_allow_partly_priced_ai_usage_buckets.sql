-- From payload version 3, a bucket with unpriced records can carry the cost of
-- its priced records: the cost is null only when no record is priced. Every
-- existing row already satisfies the looser check.
ALTER TABLE ai_usage_buckets DROP CONSTRAINT ai_usage_buckets_cost_check;
ALTER TABLE ai_usage_buckets ADD CONSTRAINT ai_usage_buckets_cost_check CHECK (
    estimated_cost_usd >= 0 AND (estimated_cost_usd IS NOT NULL OR unpriced_records > 0)
    );
