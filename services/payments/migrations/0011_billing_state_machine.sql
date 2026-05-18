-- Migration: 0011 — Unified Billing State Machine
--
-- Adds three industrial-grade primitives:
--   1. billing_domain + workflow_metadata on invoices  — polymorphic per-domain state
--   2. driver_ledgers                                  — persistent cash-liability ledger
--   3. driver_ledger_entries                           — immutable append-only debit/credit log
--   4. billing_clearance view                          — fast manifest-lock gate query
--
-- Design rationale:
--   - JSONB workflow_metadata carries domain-specific variables without fracturing the schema.
--   - driver_ledgers uses optimistic version locking to prevent double-debit under concurrent
--     POD events (same pattern as payments.wallets.version).
--   - driver_ledger_entries is append-only (INSERT-only rule) matching wallet_transactions.
--   - All tables are RLS-isolated per tenant via current_setting('app.tenant_id').

-- ─────────────────────────────────────────────────────────────────────────────
-- 1. Extend invoices with billing domain + polymorphic workflow state
-- ─────────────────────────────────────────────────────────────────────────────

ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS billing_domain TEXT
        CHECK (billing_domain IN ('balikbayan', 'standard_parcel', 'cod', 'merchant_periodic'))
        DEFAULT 'merchant_periodic',

    -- Polymorphic per-domain variables.
    -- balikbayan Stage 1: {"stage":"deposit","box_id":"...","deposit_amount_cents":5000}
    -- balikbayan Stage 2: {"stage":"final","box_size":"large","ocean_freight_cents":50000,
    --                      "deposit_deducted_cents":5000,"stage_1_invoice_id":"<uuid>"}
    -- standard_parcel:    {"declared_weight_grams":500,"actual_weight_grams":750,
    --                      "overage_surcharge_cents":2500,"held_since":"2026-05-17T10:00:00Z"}
    -- cod:                {"collected_cents":15000,"payment_method":"cash",
    --                      "collected_at":"2026-05-17T10:30:00Z"}
    ADD COLUMN IF NOT EXISTS workflow_metadata JSONB NOT NULL DEFAULT '{}';

-- GIN index — fast lookup of domain-specific JSONB fields (e.g., box_id, stage_1_invoice_id)
CREATE INDEX IF NOT EXISTS idx_invoices_billing_domain
    ON payments.invoices (tenant_id, billing_domain)
    WHERE billing_domain IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_invoices_workflow_metadata
    ON payments.invoices USING GIN (workflow_metadata)
    WHERE workflow_metadata <> '{}';

-- ─────────────────────────────────────────────────────────────────────────────
-- 2. driver_ledgers — one active ledger per (tenant, driver, shift)
--
-- balance_cents convention:
--   Negative  = driver owes the platform (has collected but not remitted cash)
--   Zero      = clean (no outstanding cash liability)
--   Positive  = platform owes driver (credit: overpayment, bonus, etc.)
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payments.driver_ledgers (
    id                      UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               UUID        NOT NULL,
    driver_id               UUID        NOT NULL,

    -- shift_id is NULL for a "lifetime" ledger (pre-shift-management era).
    -- Non-null once shift management is enabled — one ledger per shift.
    shift_id                UUID,

    -- Running net balance in the tenant's base currency minor unit (cents/centavos).
    -- Protected by optimistic version locking — never update without WHERE version = $N.
    balance_cents           BIGINT      NOT NULL DEFAULT 0,
    currency                TEXT        NOT NULL DEFAULT 'PHP',

    -- Reconciliation lifecycle
    reconciliation_status   TEXT        NOT NULL DEFAULT 'pending'
                            CHECK (reconciliation_status IN ('pending','reconciled','disputed')),
    last_reconciliation_at  TIMESTAMPTZ,
    reconciled_by           UUID,           -- hub manager who executed the reconcile event

    -- Optimistic concurrency lock — increment on every balance mutation.
    -- Matches the pattern in payments.wallets.
    version                 INT         NOT NULL DEFAULT 1,

    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One open (pending) ledger per driver per shift.
    CONSTRAINT uq_driver_ledger_shift UNIQUE (tenant_id, driver_id, shift_id)
);

CREATE INDEX IF NOT EXISTS idx_driver_ledgers_driver
    ON payments.driver_ledgers (tenant_id, driver_id, reconciliation_status);

ALTER TABLE payments.driver_ledgers ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.driver_ledgers
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─────────────────────────────────────────────────────────────────────────────
-- 3. driver_ledger_entries — immutable audit trail
--
-- Every cash debit or credit is a permanent row; no UPDATE/DELETE allowed.
-- Mirrors the wallet_transactions immutability pattern.
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payments.driver_ledger_entries (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id       UUID        NOT NULL
                                REFERENCES payments.driver_ledgers(id)
                                ON DELETE RESTRICT,
    tenant_id       UUID        NOT NULL,

    -- Entry classification
    entry_type      TEXT        NOT NULL
                    CHECK (entry_type IN (
                        -- Cash debits (driver owes platform)
                        'balikbayan_deposit_collected',   -- Track A Stage 1 cash in hand
                        'balikbayan_final_collected',     -- Track A Stage 2 cash in hand
                        'cod_collected',                  -- Track C doorstep COD cash
                        -- Cash credits (reduces liability)
                        'cash_remitted_to_hub',           -- Driver handed cash to hub manager
                        'cash_reconciled',                -- Finance confirmed reconciliation
                        -- Adjustments
                        'overage_reversal',               -- Returned change, QR wallet refund
                        'manual_adjustment'               -- Admin corrective entry
                    )),

    -- Signed amount: negative = increases liability, positive = reduces liability.
    -- e.g., cod_collected => -15000 (driver owes 150 PHP)
    --       cash_remitted  => +15000 (liability cleared)
    amount_cents    BIGINT      NOT NULL,
    currency        TEXT        NOT NULL DEFAULT 'PHP',

    -- Causation references
    shipment_id     UUID,
    invoice_id      UUID,
    pod_id          UUID,

    -- Freetext memo for audit (e.g., "COD AWB CM-UAE1-B0001234X, collected on doorstep")
    memo            TEXT        NOT NULL DEFAULT '',

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()

    -- No updated_at — this table is append-only.
);

-- Prevent any UPDATE or DELETE (immutable ledger)
CREATE RULE no_update_ledger_entries AS
    ON UPDATE TO payments.driver_ledger_entries DO INSTEAD NOTHING;
CREATE RULE no_delete_ledger_entries AS
    ON DELETE TO payments.driver_ledger_entries DO INSTEAD NOTHING;

CREATE INDEX IF NOT EXISTS idx_ledger_entries_ledger
    ON payments.driver_ledger_entries (ledger_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ledger_entries_shipment
    ON payments.driver_ledger_entries (tenant_id, shipment_id)
    WHERE shipment_id IS NOT NULL;

ALTER TABLE payments.driver_ledger_entries ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.driver_ledger_entries
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─────────────────────────────────────────────────────────────────────────────
-- 4. Extend charge_type CHECK to include new billing-domain charges
-- ─────────────────────────────────────────────────────────────────────────────

ALTER TABLE payments.invoice_line_items
    DROP CONSTRAINT IF EXISTS invoice_line_items_charge_type_check;

ALTER TABLE payments.invoice_line_items
    ADD CONSTRAINT invoice_line_items_charge_type_check
    CHECK (charge_type IN (
        -- Existing
        'base_freight', 'weight_surcharge', 'dimensional_surcharge',
        'remote_area_surcharge', 'fuel_surcharge', 'cod_handling_fee',
        'failed_delivery_fee', 'return_fee', 'insurance_fee',
        'customs_duty', 'storage_fee', 'reschedule_fee', 'manual_adjustment',
        -- Track A — Balikbayan
        'balikbayan_deposit',       -- Stage 1: flat AED deposit collected at empty-box delivery
        'balikbayan_ocean_freight', -- Stage 2: flat ocean freight rate by box size
        'balikbayan_deposit_credit',-- Stage 2: negative line-item crediting back the deposit
        -- Track B — Standard Parcel
        'hub_inbound_freight',      -- Triggered at Hub Inbound milestone
        'weight_overage_surcharge', -- Auto-generated when hub scale detects mismatch
        -- Track C — COD (supplement)
        'cod_collection_fee'        -- Platform handling fee on cash collected
    ));

-- ─────────────────────────────────────────────────────────────────────────────
-- 5. billing_clearance view — manifest lock gate (zero-join, fast path)
--
-- Used by: Dispatch before sea-container assignment (Balikbayan)
--          Hub-Ops before container seal
--          Driver-Ops before package release (Standard Parcel overage hold)
--
-- A shipment is "cleared" iff:
--   - No unpaid invoices exist for that shipment, OR
--   - The shipment's billing_domain is neither 'balikbayan' nor 'standard_parcel'
--     (COD clearance is enforced at the mobile layer, not the manifest layer).
-- ─────────────────────────────────────────────────────────────────────────────

CREATE OR REPLACE VIEW payments.billing_clearance AS
SELECT
    (inv.workflow_metadata->>'shipment_id')::UUID   AS shipment_id,
    inv.tenant_id,
    inv.billing_domain,
    -- true = all outstanding invoices paid; false = at least one unpaid
    BOOL_AND(inv.status = 'paid')                   AS is_cleared,
    COUNT(*) FILTER (WHERE inv.status <> 'paid')    AS unpaid_invoice_count,
    ARRAY_AGG(inv.id) FILTER (WHERE inv.status <> 'paid')
                                                    AS unpaid_invoice_ids
FROM payments.invoices inv
WHERE inv.billing_domain IN ('balikbayan', 'standard_parcel')
  AND inv.workflow_metadata ? 'shipment_id'
GROUP BY
    (inv.workflow_metadata->>'shipment_id')::UUID,
    inv.tenant_id,
    inv.billing_domain;

COMMENT ON VIEW payments.billing_clearance IS
    'Manifest-lock gate: join on shipment_id + tenant_id to check is_cleared before '
    'container assignment (Balikbayan) or delivery release (StandardParcel overage hold). '
    'Never perform a full table scan — always filter by tenant_id.';
