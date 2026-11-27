-- Migration: 0018 — the survey's addendum
--
-- Decided 2026-09-19: the surveyor (the lead) lists what the survey found
-- beyond the booked inventory — more items, packing materials, resources —
-- and only ever adds: the price the customer agreed never goes down. The
-- customer approves or declines it in the app; approval charges the
-- difference, as its own payment. One addendum is open at a time.

CREATE TABLE IF NOT EXISTS order_intake.home_addenda (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    shipment_id       UUID        NOT NULL,
    tenant_id         UUID        NOT NULL,
    submitted_by      UUID        NOT NULL,
    -- Catalogue items found beyond the booking, as priced.
    items             JSONB       NOT NULL DEFAULT '[]',
    -- Materials and resources: [{kind, name, qty, unit_cents}].
    extras            JSONB       NOT NULL DEFAULT '[]',
    note              TEXT        NOT NULL DEFAULT '',
    items_cents       BIGINT      NOT NULL CHECK (items_cents >= 0),
    extras_cents      BIGINT      NOT NULL CHECK (extras_cents >= 0),
    total_cents       BIGINT      NOT NULL CHECK (total_cents > 0),
    currency          TEXT        NOT NULL,
    -- The crew and trucks the whole move needs with the additions.
    trucks            INTEGER     NOT NULL,
    helpers           INTEGER     NOT NULL,
    crew_total        INTEGER     NOT NULL,
    large_estate      BOOLEAN     NOT NULL,
    status            TEXT        NOT NULL DEFAULT 'pending'
                      CHECK (status IN ('pending', 'approved', 'declined', 'paid')),
    payment_intent_id UUID,
    checkout_url      TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at        TIMESTAMPTZ,
    paid_at           TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_home_addenda_one_open
    ON order_intake.home_addenda (shipment_id) WHERE status IN ('pending', 'approved');
CREATE INDEX IF NOT EXISTS idx_home_addenda_shipment ON order_intake.home_addenda (shipment_id, created_at DESC);
