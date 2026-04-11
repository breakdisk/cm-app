-- Migration: 0003 — Payments: restructure invoices with structured number,
--             normalized line items, and adjustments table.
--
-- Before this migration invoices stored line_items as JSONB.
-- After: line items are a first-class relational table, keyed by charge_type
-- and AWB, enabling per-AWB billing queries without JSON parsing.

-- ── Step 1: add new columns to invoices ──────────────────────────────────────
ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS invoice_number  TEXT,
    ADD COLUMN IF NOT EXISTS invoice_type    TEXT NOT NULL DEFAULT 'shipment_charges'
                                             CHECK (invoice_type IN (
                                                 'shipment_charges','cod_remittance',
                                                 'credit_note','wallet_top_up','carrier_payable'
                                             )),
    ADD COLUMN IF NOT EXISTS billing_period_start  DATE,
    ADD COLUMN IF NOT EXISTS billing_period_end    DATE,
    ADD COLUMN IF NOT EXISTS created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Back-fill invoice_number for existing rows (synthetic — non-structured)
UPDATE payments.invoices
    SET invoice_number = 'IN-LEG-' ||
        TO_CHAR(issued_at, 'YYYY-MM') || '-' ||
        LPAD(CAST(ROW_NUMBER() OVER (ORDER BY issued_at) AS TEXT), 5, '0')
    WHERE invoice_number IS NULL;

ALTER TABLE payments.invoices
    ALTER COLUMN invoice_number SET NOT NULL;

ALTER TABLE payments.invoices
    ADD CONSTRAINT invoices_number_unique UNIQUE (invoice_number);

-- ── Step 2: invoice_line_items (replaces line_items JSONB column) ─────────────

CREATE TABLE IF NOT EXISTS payments.invoice_line_items (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id      UUID        NOT NULL
                                REFERENCES payments.invoices(id)
                                ON DELETE CASCADE,
    tenant_id       UUID        NOT NULL,

    charge_type     TEXT        NOT NULL
                                CHECK (charge_type IN (
                                    'base_freight','weight_surcharge','dimensional_surcharge',
                                    'remote_area_surcharge','fuel_surcharge','cod_handling_fee',
                                    'failed_delivery_fee','return_fee','insurance_fee',
                                    'customs_duty','storage_fee','reschedule_fee',
                                    'manual_adjustment'
                                )),

    -- AWB reference — NULL for document-level charges (fuel, manual adjustments)
    awb             TEXT,

    description     TEXT        NOT NULL,
    quantity        INTEGER     NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit_price_cents BIGINT     NOT NULL,
    currency        TEXT        NOT NULL DEFAULT 'PHP',

    -- Optional discount on this line
    discount_cents  BIGINT      DEFAULT 0,
    -- Required when charge_type = 'manual_adjustment'
    reason          TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_line_items_invoice   ON payments.invoice_line_items (invoice_id);
CREATE INDEX IF NOT EXISTS idx_line_items_awb       ON payments.invoice_line_items (awb)
    WHERE awb IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_line_items_tenant    ON payments.invoice_line_items (tenant_id, charge_type);

ALTER TABLE payments.invoice_line_items ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON payments.invoice_line_items;
CREATE POLICY tenant_isolation ON payments.invoice_line_items
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ── Step 3: migrate existing JSONB line_items → relational rows ───────────────
-- (Best-effort — only runs if existing rows have non-empty JSONB line_items)
INSERT INTO payments.invoice_line_items (
    invoice_id, tenant_id, charge_type, description,
    quantity, unit_price_cents, currency
)
SELECT
    inv.id,
    inv.tenant_id,
    'base_freight',
    COALESCE(item->>'description', 'Migrated line item'),
    COALESCE((item->>'quantity')::integer, 1),
    COALESCE((item->>'unit_price_cents')::bigint,
             (item->'unit_price'->>'amount')::bigint, 0),
    inv.currency
FROM payments.invoices inv,
     LATERAL jsonb_array_elements(
         CASE WHEN jsonb_typeof(inv.line_items) = 'array'
              THEN inv.line_items ELSE '[]'::jsonb END
     ) AS item
WHERE jsonb_typeof(inv.line_items) = 'array'
  AND jsonb_array_length(inv.line_items) > 0
ON CONFLICT DO NOTHING;

-- Keep line_items JSONB for now (soft deprecation) — will be dropped in 0004
COMMENT ON COLUMN payments.invoices.line_items IS
    'DEPRECATED: use invoice_line_items table. Will be dropped in migration 0004.';

-- ── Step 4: invoice_adjustments ───────────────────────────────────────────────
-- Post-issue credits/debits (weight discrepancies, manual credits, etc.)

CREATE TABLE IF NOT EXISTS payments.invoice_adjustments (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id      UUID        NOT NULL
                                REFERENCES payments.invoices(id)
                                ON DELETE CASCADE,
    tenant_id       UUID        NOT NULL,

    charge_type     TEXT        NOT NULL,
    -- Positive = additional charge; negative = credit to merchant
    amount_cents    BIGINT      NOT NULL,
    currency        TEXT        NOT NULL DEFAULT 'PHP',

    reason          TEXT        NOT NULL,
    -- AWB that triggered the adjustment (e.g. weight discrepancy)
    awb             TEXT,

    created_by      UUID        NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_adjustments_invoice ON payments.invoice_adjustments (invoice_id);
CREATE INDEX IF NOT EXISTS idx_adjustments_awb     ON payments.invoice_adjustments (awb)
    WHERE awb IS NOT NULL;

ALTER TABLE payments.invoice_adjustments ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON payments.invoice_adjustments;
CREATE POLICY tenant_isolation ON payments.invoice_adjustments
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ── Step 5: invoice_number sequence counters ──────────────────────────────────
-- Mirrors Redis `inv:seq:{prefix}:{tenant}:{YYYY}-{MM}` for DR / reconciliation.

CREATE TABLE IF NOT EXISTS payments.invoice_sequences (
    prefix          TEXT        NOT NULL CHECK (prefix IN ('IN','REM','CN','WR','CP')),
    tenant_code     CHAR(3)     NOT NULL,
    period          CHAR(7)     NOT NULL,  -- 'YYYY-MM'
    last_issued     INTEGER     NOT NULL DEFAULT 0
                                CHECK (last_issued BETWEEN 0 AND 99999),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (prefix, tenant_code, period)
);

COMMENT ON TABLE payments.invoice_sequences IS
    'High-water mark for invoice sequences per prefix/tenant/month. '
    'The live counter lives in Redis (inv:seq:{prefix}:{tenant}:{YYYY}-{MM}). '
    'Updated on Redis failover and periodic reconciliation.';

-- ── Step 6: useful view for per-AWB billing summary ──────────────────────────

CREATE OR REPLACE VIEW payments.awb_charges AS
SELECT
    li.awb,
    li.tenant_id,
    inv.merchant_id,
    inv.invoice_number,
    inv.billing_period_start,
    li.charge_type,
    li.quantity,
    li.unit_price_cents,
    li.discount_cents,
    (li.quantity * li.unit_price_cents - COALESCE(li.discount_cents, 0)) AS net_cents,
    li.currency,
    inv.status  AS invoice_status
FROM payments.invoice_line_items li
JOIN payments.invoices inv ON inv.id = li.invoice_id
WHERE li.awb IS NOT NULL;

COMMENT ON VIEW payments.awb_charges IS
    'Per-AWB charge breakdown across all invoices. '
    'Join to order_intake.shipments on awb for full shipment billing view.';
