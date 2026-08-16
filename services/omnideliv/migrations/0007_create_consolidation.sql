-- A courier route over a multi-vendor basket. Stops are ordered by readiness,
-- not distance — see the sequencing rule in the entity.
CREATE TABLE IF NOT EXISTS omnideliv.consolidation_plans (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    basket_id           UUID        NOT NULL REFERENCES omnideliv.baskets(id),
    -- Ordered stops: [{"vendor_id": ..., "seq": 0, ...}, ...]
    stops               JSONB       NOT NULL DEFAULT '[]',
    total_distance_m    INT         NOT NULL DEFAULT 0,
    -- One fee for the whole route, whatever the stop count. The product promise
    -- and the margin lever in the same column.
    flat_fee_cents      BIGINT      NOT NULL CHECK (flat_fee_cents >= 0),
    -- ["hot","chilled"] etc. Populated when a basket mixes classes, so ops can
    -- see why a route was sequenced the way it was.
    temperature_classes TEXT[]      NOT NULL DEFAULT '{}',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_consolidation_basket
    ON omnideliv.consolidation_plans (basket_id);
