-- Courier earnings. Same append-only shape as the vendor ledger and the
-- platform's existing DriverLedger — one pattern for money across all three.
--
-- ADR-0015 deferred this out of the minimal extraction until OmniDeliv's
-- settlement model existed, on the grounds that building it first would mean
-- guessing at the shape. It exists now, so this follows it.
CREATE TABLE IF NOT EXISTS field_ops.courier_ledgers (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    courier_id    UUID        NOT NULL REFERENCES field_ops.couriers(id),
    -- Shift or payout period.
    period        TEXT        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open','closed','settled')),
    balance_cents BIGINT      NOT NULL DEFAULT 0,
    version       BIGINT      NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_ledger_period
    ON field_ops.courier_ledgers (tenant_id, courier_id, period);

CREATE TABLE IF NOT EXISTS field_ops.courier_ledger_entries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id    UUID        NOT NULL REFERENCES field_ops.courier_ledgers(id),
    kind         TEXT        NOT NULL
                             CHECK (kind IN ('trip_earning','tip','adjustment','payout')),
    -- Signed: credits positive, payouts negative, so the balance is a plain SUM.
    amount_cents BIGINT      NOT NULL,
    -- The product's own job id. field-ops does not interpret it — the same
    -- opacity that keeps this tier product-agnostic in courier_assignments.
    external_ref UUID,
    reference    TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_courier_entry_ledger
    ON field_ops.courier_ledger_entries (ledger_id, created_at);

-- Append-only, stated. See the note in omnideliv's 0009: services connect as
-- the schema owner, so this does not bind today and starts enforcing when they
-- do not. The entity exposing no method that edits an entry is what enforces it
-- in the meantime.
REVOKE UPDATE, DELETE ON field_ops.courier_ledger_entries FROM PUBLIC;
