-- Orders and their per-vendor settlement legs.
--
-- Every money column is BIGINT cents. No NUMERIC and certainly no float: a
-- rounding error here is money created or destroyed.
CREATE TABLE IF NOT EXISTS omnideliv.orders (
    id                 UUID        PRIMARY KEY,
    tenant_id          UUID        NOT NULL,
    customer_id        UUID        NOT NULL,
    basket_id          UUID        NOT NULL REFERENCES omnideliv.baskets(id),
    plan_id            UUID        NOT NULL,
    status             TEXT        NOT NULL DEFAULT 'placed'
                                   CHECK (status IN ('placed','awaiting_courier','collecting','delivering','delivered','cancelled')),
    goods_total_cents  BIGINT      NOT NULL CHECK (goods_total_cents >= 0),
    delivery_fee_cents BIGINT      NOT NULL CHECK (delivery_fee_cents >= 0),
    tip_cents          BIGINT      NOT NULL DEFAULT 0 CHECK (tip_cents >= 0),
    grand_total_cents  BIGINT      NOT NULL CHECK (grand_total_cents >= 0),
    -- What the courier is paid for the trip. Deliberately NOT constrained to be
    -- at most the fee: a trip costing more than the fee is a loss-leading order
    -- under a short-distance pricing floor, which is a pricing decision rather
    -- than a data error.
    courier_trip_cents BIGINT      NOT NULL DEFAULT 0 CHECK (courier_trip_cents >= 0),
    courier_task_id    UUID,
    placed_at          TIMESTAMPTZ NOT NULL,
    delivered_at       TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_order_customer
    ON omnideliv.orders (tenant_id, customer_id, placed_at DESC);

CREATE INDEX IF NOT EXISTS idx_order_basket
    ON omnideliv.orders (basket_id);

CREATE TABLE IF NOT EXISTS omnideliv.order_vendor_legs (
    id                   UUID        PRIMARY KEY,
    order_id             UUID        NOT NULL REFERENCES omnideliv.orders(id) ON DELETE CASCADE,
    tenant_id            UUID        NOT NULL,
    vendor_id            UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    goods_subtotal_cents BIGINT      NOT NULL CHECK (goods_subtotal_cents >= 0),
    -- Snapshotted at order time. The vendor's rate may change later; this order
    -- settles at the rate that applied when it was placed.
    commission_bps       INT         NOT NULL CHECK (commission_bps BETWEEN 0 AND 10000),
    commission_cents     BIGINT      NOT NULL CHECK (commission_cents >= 0),
    payout_cents         BIGINT      NOT NULL CHECK (payout_cents >= 0),
    status               TEXT        NOT NULL DEFAULT 'pending'
                                     CHECK (status IN ('pending','picked_up','failed','settled')),
    picked_up_at         TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL,

    -- The per-leg half of the balance invariant, enforced by the database.
    -- The application asserts this too; having it here means a bad write from
    -- any future path fails loudly instead of quietly unbalancing the ledger.
    CONSTRAINT leg_splits_exactly
        CHECK (commission_cents + payout_cents = goods_subtotal_cents)
);

CREATE INDEX IF NOT EXISTS idx_leg_order  ON omnideliv.order_vendor_legs (order_id);
CREATE INDEX IF NOT EXISTS idx_leg_vendor ON omnideliv.order_vendor_legs (tenant_id, vendor_id);
