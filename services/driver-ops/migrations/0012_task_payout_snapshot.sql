-- Contractual payout snapshot: copied from the offer/assignment at creation.
-- The price shown to the gig driver at grab time is what they are paid —
-- later rate changes never alter accepted work. Earnings history reads this
-- column; NULL means "no contractual price" (full-time driver or pre-snapshot
-- row, where the live per_delivery_rate_cents remains the display fallback).
ALTER TABLE driver_ops.tasks ADD COLUMN IF NOT EXISTS payout_cents BIGINT;

-- Earnings range queries: SUM / GROUP BY day over (driver, completed window).
-- Reviewed alternative (async rollup table) rejected as YAGNI — an indexed
-- range aggregate over a driver's task history is single-digit ms at any
-- realistic volume (~11k rows/driver/year).
CREATE INDEX IF NOT EXISTS idx_tasks_driver_status_completed
    ON driver_ops.tasks (driver_id, status, completed_at);
