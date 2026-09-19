-- Migration: 0014 — how a booking was described
--
-- The Move app turns a sentence ("move my sofa and 6 boxes to Makati
-- tomorrow") into a plan the customer then corrects. Recording what was read
-- from the sentence beside the shipment that was booked is how parse quality
-- is measured: what the reader got right is what the customer left alone.
--
-- Its own table rather than columns on shipments: it is written once, read
-- only by analysis, and a booking made from a form has none.

CREATE TABLE IF NOT EXISTS order_intake.shipment_intake (
    shipment_id UUID        PRIMARY KEY,
    tenant_id   UUID        NOT NULL,
    -- prompt_ai | prompt_offline | voice_ai | voice_offline
    source      TEXT        NOT NULL,
    -- book | home_move | support, when the server classified it
    intent      TEXT,
    confidence  DOUBLE PRECISION,
    -- { items: [{name, qty}], from, to, when } as read, before any correction
    extracted   JSONB       NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_shipment_intake_tenant_source
    ON order_intake.shipment_intake (tenant_id, source, created_at DESC);
