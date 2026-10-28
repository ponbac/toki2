-- The pricing basis the client priced a bucket's cost with: an opaque string
-- that names the pricing method and the rates it used, compared only for
-- equality. Every upload stores its basis on each bucket it writes, and a
-- reprice-only upload replaces the cost and basis of buckets it reproduces
-- exactly, so stored costs from an older method can be found per bucket. NULL
-- for buckets from clients that report no basis.
ALTER TABLE ai_usage_buckets ADD COLUMN pricing_basis TEXT;
ALTER TABLE ai_usage_buckets ADD CONSTRAINT ai_usage_buckets_pricing_basis_check
    CHECK (char_length(pricing_basis) BETWEEN 1 AND 128);
