-- Vendor leg acceptance — ADR-0017.
--
-- The store is asked before the courier is sent. Until now a leg went straight
-- from 'pending' to 'picked_up', because nobody ever asked the vendor anything:
-- a restaurant found out it had an order when a courier walked in.
--
-- The status CHECK in 0008 is an inline, unnamed column constraint, so Postgres
-- named it `order_vendor_legs_status_check`. It has to be dropped and rebuilt
-- rather than extended — a CHECK cannot be altered in place.

ALTER TABLE omnideliv.order_vendor_legs
    DROP CONSTRAINT IF EXISTS order_vendor_legs_status_check;

ALTER TABLE omnideliv.order_vendor_legs
    ADD CONSTRAINT order_vendor_legs_status_check
    CHECK (status IN (
        'pending', 'accepted', 'preparing', 'ready',
        'picked_up', 'served', 'rejected', 'failed', 'settled'
    ));

ALTER TABLE omnideliv.order_vendor_legs
    ADD COLUMN IF NOT EXISTS accepted_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ready_at         TIMESTAMPTZ,
    -- What the store promised when it accepted. This is the basis for a real
    -- ready time instead of the `vendors.prep_time_minutes` guess, which is a
    -- static per-store default that nothing ever reconciles against reality.
    ADD COLUMN IF NOT EXISTS ready_in_minutes INT,
    ADD COLUMN IF NOT EXISTS rejected_reason  TEXT;

-- Bound the promise. An unbounded value silently becomes an SLA nobody agreed
-- to, and a negative one would make `ready_at` precede `accepted_at`. The API
-- checks this too; having it here means a bad write from any future path fails
-- loudly rather than quietly, which is the same reasoning as
-- `leg_splits_exactly` in 0008.
ALTER TABLE omnideliv.order_vendor_legs
    DROP CONSTRAINT IF EXISTS leg_ready_in_minutes_sane;

ALTER TABLE omnideliv.order_vendor_legs
    ADD CONSTRAINT leg_ready_in_minutes_sane
    CHECK (ready_in_minutes IS NULL OR ready_in_minutes BETWEEN 1 AND 240);

-- The vendor queue's only query:
--     WHERE tenant_id = $1 AND vendor_id = $2 AND status IN (<live>)
--     ORDER BY created_at ASC
--
-- Leads with tenant_id because every query in this service filters on it
-- (ADR-0016 — tenant isolation is the repository signature, not RLS), so an
-- index that omits it cannot serve the leading predicate. Partial, because the
-- queue only ever reads live legs and the settled history is the larger part of
-- the table over time. `created_at` is in the index so the ORDER BY is free —
-- a kitchen works its queue in the order it arrived.
CREATE INDEX IF NOT EXISTS idx_leg_vendor_open
    ON omnideliv.order_vendor_legs (tenant_id, vendor_id, created_at)
    WHERE status IN ('pending', 'accepted', 'preparing', 'ready');
