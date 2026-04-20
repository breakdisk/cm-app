-- Migration: 0007 — Track auto-dispatch attempts so the admin console can
-- surface shipments where the first auto-dispatch attempt failed (e.g. no
-- available drivers in zone, compliance block, stale driver location).
--
-- Before this migration, a failed auto-dispatch just logged a warning and
-- left the queue row in `status=pending` — visually indistinguishable from
-- a brand-new pending shipment. Ops had no signal that human intervention
-- was needed.

ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS auto_dispatch_attempts INT         NOT NULL DEFAULT 0;
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS last_dispatch_error    TEXT;
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS last_attempt_at        TIMESTAMPTZ;

-- Partial index to make "show me stuck shipments" queries cheap.
CREATE INDEX IF NOT EXISTS idx_dispatch_queue_failed_attempts
    ON dispatch.dispatch_queue (tenant_id, last_attempt_at DESC)
    WHERE auto_dispatch_attempts > 0 AND status = 'pending';
