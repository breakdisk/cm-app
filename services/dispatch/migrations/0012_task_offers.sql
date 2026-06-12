-- Gig offer broadcast ("grab") — contended offer object + per-candidate fan-out.
--
-- task_offers is the single row hundreds of drivers race on: the claim is a
-- compare-and-swap UPDATE on status='open'. Card display fields are
-- denormalized here so GET /v1/offers/open needs no dispatch_queue join.
CREATE TABLE IF NOT EXISTS dispatch.task_offers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    shipment_id   UUID NOT NULL,
    queue_id      UUID NOT NULL,
    status        TEXT NOT NULL DEFAULT 'open',   -- open|claimed|expired|cancelled
    wave          INT  NOT NULL DEFAULT 1,        -- 1..3, radius widens per wave
    expires_at    TIMESTAMPTZ NOT NULL,
    claimed_by    UUID,
    claimed_at    TIMESTAMPTZ,
    -- Denormalized card fields (snapshot from dispatch_queue at creation)
    merchant_name     TEXT   NOT NULL DEFAULT '',
    delivery_category TEXT   NOT NULL DEFAULT 'parcel',
    weight_grams      BIGINT NOT NULL DEFAULT 0,
    tracking_number   TEXT   NOT NULL DEFAULT '',
    customer_name     TEXT   NOT NULL DEFAULT '',
    pickup_address    TEXT   NOT NULL DEFAULT '',
    delivery_address  TEXT   NOT NULL DEFAULT '',
    cod_amount_cents  BIGINT,
    pickup_lat        DOUBLE PRECISION,
    pickup_lng        DOUBLE PRECISION,
    delivery_lat      DOUBLE PRECISION,
    delivery_lng      DOUBLE PRECISION,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Sweeper scan: expired-but-still-open offers.
CREATE INDEX IF NOT EXISTS idx_offers_status_expires
    ON dispatch.task_offers (status, expires_at);
-- One open offer per shipment at a time (re-broadcast reuses the row).
CREATE UNIQUE INDEX IF NOT EXISTS idx_offers_open_per_shipment
    ON dispatch.task_offers (shipment_id) WHERE status = 'open';

-- Per-candidate fan-out record. payout_cents is snapshotted PER DRIVER from
-- their per_delivery_rate_cents (rates differ per driver) — each candidate
-- sees their own contractual price; the winner's is copied to the task.
-- seen_at is the client-verified impression (POST /v1/offers/:id/seen fired
-- at card render) — FCM delivery alone is NOT an impression; acceptance-rate
-- denominators count only verified impressions so dead zones don't penalize.
CREATE TABLE IF NOT EXISTS dispatch.task_offer_candidates (
    offer_id     UUID NOT NULL REFERENCES dispatch.task_offers(id) ON DELETE CASCADE,
    driver_id    UUID NOT NULL,
    wave         INT  NOT NULL DEFAULT 1,
    payout_cents BIGINT,
    notified_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    seen_at      TIMESTAMPTZ,
    response     TEXT NOT NULL DEFAULT 'none',    -- none|passed
    PRIMARY KEY (offer_id, driver_id)
);

CREATE INDEX IF NOT EXISTS idx_offer_candidates_driver
    ON dispatch.task_offer_candidates (driver_id);

-- ── One-active-assignment invariant ─────────────────────────────────────────
-- Closes the concurrent double-grab race: two simultaneous claims by the same
-- driver on DIFFERENT offers both pass the read-guard under READ COMMITTED
-- (neither sees the other's uncommitted insert). The same check-then-insert
-- race exists latently in create_route_plan / assign_driver / quick_dispatch.
-- The losing transaction now fails deterministically with 23505.
--
-- Cleanup first: cancel pre-existing duplicate active assignments (keep the
-- newest) so the index can build on legacy data.
WITH ranked AS (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY driver_id ORDER BY assigned_at DESC) AS rn
    FROM dispatch.driver_assignments
    WHERE status IN ('pending', 'accepted')
)
UPDATE dispatch.driver_assignments a
SET status = 'cancelled'
FROM ranked r
WHERE a.id = r.id AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_one_active_assignment_per_driver
    ON dispatch.driver_assignments (driver_id)
    WHERE status IN ('pending', 'accepted');
