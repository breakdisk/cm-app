-- Migration 0004: Align invoices table with updated domain model.
--
-- Changes:
-- 1. Add billing_start / billing_end aliases for the billing period columns
--    (old migration used billing_period_start / billing_period_end).
-- 2. Add adjustments JSONB column (inline storage vs separate table).
-- 3. Add updated_at column.
-- 4. Extend invoice_type CHECK to include 'payment_receipt'.

-- ── billing period column aliases ─────────────────────────────────────────────
ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS billing_start  DATE,
    ADD COLUMN IF NOT EXISTS billing_end    DATE;

-- Back-fill from old column names if they exist
UPDATE payments.invoices
    SET billing_start = billing_period_start,
        billing_end   = billing_period_end
    WHERE billing_start IS NULL
      AND billing_period_start IS NOT NULL;

-- Default any still-null rows to today
UPDATE payments.invoices
    SET billing_start = issued_at::DATE,
        billing_end   = issued_at::DATE
    WHERE billing_start IS NULL;

ALTER TABLE payments.invoices
    ALTER COLUMN billing_start SET NOT NULL,
    ALTER COLUMN billing_end   SET NOT NULL;

-- ── adjustments JSONB (denormalised for read performance) ────────────────────
ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS adjustments JSONB NOT NULL DEFAULT '[]'::JSONB;

-- ── updated_at ────────────────────────────────────────────────────────────────
ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- ── extend invoice_type to include payment_receipt ───────────────────────────
-- Drop the old CHECK and add the new one
ALTER TABLE payments.invoices
    DROP CONSTRAINT IF EXISTS invoices_invoice_type_check;

ALTER TABLE payments.invoices
    ADD CONSTRAINT invoices_invoice_type_check
    CHECK (invoice_type IN (
        'shipment_charges', 'cod_remittance', 'credit_note',
        'wallet_top_up', 'carrier_payable', 'payment_receipt', 'other'
    ));
