-- Migration 0011: Driver cash-flow ledger for Track A (Balikbayan) pickup debits
-- and COD collected at delivery.
--
-- A DriverLedger is opened per driver per shift. When the driver picks up a
-- Balikbayan parcel the payments service debits `declared_value_cents` here so
-- finance can see how much cash each driver is liable for before they remit.
--
-- Remittance creates a credit entry (negative amount_cents). When balance_cents
-- reaches 0, the ledger is flipped to 'reconciled'.

CREATE TABLE IF NOT EXISTS payments.driver_ledgers (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    driver_id     UUID        NOT NULL,
    -- NULL shift_id for drivers without formal shift management.
    shift_id      UUID,
    status        TEXT        NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open', 'reconciled')),
    balance_cents BIGINT      NOT NULL DEFAULT 0,
    version       BIGINT      NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Unique open ledger per (tenant, driver, shift).
-- COALESCE maps NULL shift_id to a sentinel UUID so the unique constraint works.
CREATE UNIQUE INDEX IF NOT EXISTS driver_ledgers_open_shift_uq
    ON payments.driver_ledgers (
        tenant_id,
        driver_id,
        COALESCE(shift_id, '00000000-0000-0000-0000-000000000000')
    )
    WHERE status = 'open';

CREATE INDEX IF NOT EXISTS driver_ledgers_driver_idx
    ON payments.driver_ledgers (tenant_id, driver_id, status);

-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payments.driver_ledger_entries (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id     UUID        NOT NULL REFERENCES payments.driver_ledgers(id),
    kind          TEXT        NOT NULL
                              CHECK (kind IN ('pickup_debit', 'cod_debit', 'remittance')),
    -- Positive = debit (driver received funds); negative = credit (remitted).
    amount_cents  BIGINT      NOT NULL,
    shipment_id   UUID,
    -- pop_id / pod_id / batch_id for audit trail
    reference     TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS driver_ledger_entries_ledger_idx
    ON payments.driver_ledger_entries (ledger_id, created_at);

CREATE INDEX IF NOT EXISTS driver_ledger_entries_shipment_idx
    ON payments.driver_ledger_entries (shipment_id)
    WHERE shipment_id IS NOT NULL;

-- RLS: tenants can only see their own ledgers.
ALTER TABLE payments.driver_ledgers        ENABLE ROW LEVEL SECURITY;
ALTER TABLE payments.driver_ledger_entries ENABLE ROW LEVEL SECURITY;

-- Service role bypasses RLS (same pattern as other payments tables).
CREATE POLICY driver_ledgers_service_policy
    ON payments.driver_ledgers
    USING (true)
    WITH CHECK (true);

CREATE POLICY driver_ledger_entries_service_policy
    ON payments.driver_ledger_entries
    USING (true)
    WITH CHECK (true);

COMMENT ON TABLE payments.driver_ledgers IS
    'Per-driver per-shift cash liability ledger. Debited on Track A pickup, credited on remittance.';
COMMENT ON TABLE payments.driver_ledger_entries IS
    'Individual movements on a driver ledger (pickup_debit, cod_debit, remittance).';
