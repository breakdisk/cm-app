-- Migration: 0015 — Payments: payment_intents (gateway-agnostic charge ledger)
--
-- `purpose` is intentionally an open TEXT value, not a fixed enum: this table
-- is shared by every future payment surface (subscription billing, storefront
-- checkout, truck & recovery booking), each adding its own purpose value
-- rather than a parallel table. Only 'shipping_fee' is used today.
--
-- No RLS: per migrations 0014 (identity) and 0011 (order-intake), RLS here
-- was found decorative (the connection pool never sets app.tenant_id) and is
-- not re-added.

CREATE TABLE payments.payment_intents (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             UUID        NOT NULL,
    purpose               TEXT        NOT NULL,
    reference_type        TEXT        NOT NULL,
    reference_id          UUID        NOT NULL,
    amount_cents          BIGINT      NOT NULL CHECK (amount_cents > 0),
    currency              TEXT        NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'created'
                                      CHECK (status IN (
                                          'created','pending','captured','failed','refunded','expired'
                                      )),
    gateway               TEXT        NOT NULL,
    gateway_order_ref     TEXT,
    gateway_payment_ref   TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at            TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_payment_intents_reference
    ON payments.payment_intents (reference_type, reference_id);

-- Idempotent webhook capture: a replayed webhook for the same gateway
-- transaction must not create a second row or double-process.
CREATE UNIQUE INDEX idx_payment_intents_gateway_payment_ref
    ON payments.payment_intents (gateway_payment_ref)
    WHERE gateway_payment_ref IS NOT NULL;

CREATE INDEX idx_payment_intents_tenant_status
    ON payments.payment_intents (tenant_id, status);
