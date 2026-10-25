-- The pricing basis the machine's client prices with now: the latest non-null
-- basis of its uploads and reprice-only uploads, in the order they were
-- applied. An upload without a basis keeps it. A stored bucket is on an old
-- pricing basis when its own basis is NULL or differs from its machine's; the
-- hour a bucket covers says when its usage happened, not when it was priced,
-- so the newest hour cannot tell.
ALTER TABLE ai_usage_machines ADD COLUMN pricing_basis TEXT;
ALTER TABLE ai_usage_machines ADD CONSTRAINT ai_usage_machines_pricing_basis_check
    CHECK (char_length(pricing_basis) BETWEEN 1 AND 128);

-- Machines that uploaded before this migration start from the basis of their
-- newest bucket that has one, the closest stored hint of their latest upload.
UPDATE ai_usage_machines AS machine
SET pricing_basis = newest.pricing_basis
FROM (
    SELECT DISTINCT ON (machine_id) machine_id, pricing_basis
    FROM ai_usage_buckets
    WHERE pricing_basis IS NOT NULL
    ORDER BY machine_id, hour_start DESC, pricing_basis COLLATE "C"
) AS newest
WHERE newest.machine_id = machine.id;
