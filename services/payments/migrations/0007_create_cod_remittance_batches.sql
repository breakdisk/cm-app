-- Merchant-scoped COD remittance batches.
-- One batch groups many payments.cod_collections rows for a single merchant
-- up to a cutoff date. Distinct from the (currently unused) driver-daily
-- cod_batches table from migration 0002 — that one is per-driver and per-day;
-- this one is per-merchant and represents a payout obligation.

CREATE TABLE IF NOT EXISTS payments.cod_remittance_batches (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    merchant_id         UUID        NOT NULL,
    cutoff_date         DATE        NOT NULL,
    currency            TEXT        NOT NULL DEFAULT 'PHP',
    cod_count           INTEGER     NOT NULL CHECK (cod_count >= 0),
    gross_cents         BIGINT      NOT NULL CHECK (gross_cents >= 0),
    platform_fee_cents  BIGINT      NOT NULL CHECK (platform_fee_cents >= 0),
    net_cents           BIGINT      NOT NULL CHECK (net_cents >= 0),
    status              TEXT        NOT NULL DEFAULT 'created'
                                    CHECK (status IN ('created','paid','failed')),
    failure_reason      TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    paid_at             TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_cod_batches_merchant_status
    ON payments.cod_remittance_batches(tenant_id, merchant_id, status);

CREATE INDEX IF NOT EXISTS idx_cod_batches_cutoff
    ON payments.cod_remittance_batches(tenant_id, cutoff_date DESC);

ALTER TABLE payments.cod_remittance_batches ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.cod_remittance_batches
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Add merchant_id to cod_collections so remittance batching can scope by
-- (tenant, merchant) without cross-service joins. Existing dev rows get
-- nil_uuid; new rows must be populated by the CodService at POD time.
ALTER TABLE payments.cod_collections
    ADD COLUMN IF NOT EXISTS merchant_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000000';

ALTER TABLE payments.cod_collections
    ALTER COLUMN merchant_id DROP DEFAULT;

CREATE INDEX IF NOT EXISTS idx_cod_merchant_status
    ON payments.cod_collections(tenant_id, merchant_id, status);

-- Link cod_collections.batch_id to the new batches table.
-- FK allows NULL so historical Collected/Remitted rows stay valid.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'cod_collections_batch_id_fkey'
    ) THEN
        ALTER TABLE payments.cod_collections
            ADD CONSTRAINT cod_collections_batch_id_fkey
            FOREIGN KEY (batch_id)
            REFERENCES payments.cod_remittance_batches(id)
            ON DELETE SET NULL;
    END IF;
END $$;
