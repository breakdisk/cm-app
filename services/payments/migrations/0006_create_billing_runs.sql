-- Migration: 0006 — payments.billing_runs
--
-- Idempotency + audit table for the monthly BillingAggregationService.
-- One row per (tenant, merchant, period) — rerunning the same period returns
-- the existing invoice rather than double-billing the merchant.
--
-- `invoice_id` is nullable: runs over periods with zero delivered shipments
-- are still recorded (otherwise every cron tick would refetch from order-intake).

CREATE TABLE IF NOT EXISTS payments.billing_runs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    merchant_id     UUID        NOT NULL,
    period_start    DATE        NOT NULL,
    period_end      DATE        NOT NULL,
    invoice_id      UUID
                                REFERENCES payments.invoices(id)
                                ON DELETE SET NULL,
    shipment_count  INTEGER     NOT NULL DEFAULT 0 CHECK (shipment_count >= 0),
    total_cents     BIGINT      NOT NULL DEFAULT 0 CHECK (total_cents >= 0),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT billing_runs_period_valid CHECK (period_end >= period_start),
    CONSTRAINT billing_runs_unique_period
        UNIQUE (tenant_id, merchant_id, period_start, period_end)
);

CREATE INDEX idx_billing_runs_merchant
    ON payments.billing_runs (tenant_id, merchant_id, period_start DESC);
CREATE INDEX idx_billing_runs_invoice
    ON payments.billing_runs (invoice_id) WHERE invoice_id IS NOT NULL;

ALTER TABLE payments.billing_runs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.billing_runs
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

COMMENT ON TABLE payments.billing_runs IS
    'Audit + idempotency record for monthly merchant billing aggregation. '
    'Unique on (tenant, merchant, period) so the aggregator is safe to re-run.';
