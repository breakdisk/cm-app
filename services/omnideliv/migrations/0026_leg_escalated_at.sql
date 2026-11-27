-- One escalation per leg — ADR-0017.
--
-- Found by running the sweep against a real database: it re-escalated the same
-- unanswered leg on every 60-second tick, so a leg nobody answered for an hour
-- wrote sixty telemetry rows and raised sixty alerts about one order. Ops
-- paged every minute for a single stuck kitchen is ops ignoring the alert.
--
-- Deliberately NOT part of `status`. The ladder must never change a leg's
-- status: the collection consumer refuses to credit a leg that is not awaiting
-- collection, so a status change here would stop a store being paid for food it
-- actually cooked. This is a bookkeeping stamp beside the status, not a rung of
-- the state machine, and nothing reads it except the sweep.

ALTER TABLE omnideliv.order_vendor_legs
    ADD COLUMN IF NOT EXISTS escalated_at TIMESTAMPTZ;

-- The sweep reads `status = 'pending'` and now also needs to know which of
-- those have already been raised. Folded into the existing partial index's
-- job rather than adding a second one: this is the same scan.
CREATE INDEX IF NOT EXISTS idx_leg_pending_unescalated
    ON omnideliv.order_vendor_legs (created_at)
    WHERE status = 'pending' AND escalated_at IS NULL;
