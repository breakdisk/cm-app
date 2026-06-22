-- Carrier settlement runs: batch payout records aggregated per carrier per period.
-- Created when the weekly carrier settlement cron sweeps all unsettled carrier
-- wallet transactions and generates withdrawal requests for each carrier.
CREATE TABLE IF NOT EXISTS payments.carrier_settlement_runs (
    id                         UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id                  UUID        NOT NULL,
    carrier_id                 UUID        NOT NULL,
    period_start               DATE        NOT NULL,
    period_end                 DATE        NOT NULL,
    total_margin_cents         BIGINT      NOT NULL DEFAULT 0,
    total_cod_commission_cents BIGINT      NOT NULL DEFAULT 0,
    total_cents                BIGINT      NOT NULL DEFAULT 0,
    delivery_count             INTEGER     NOT NULL DEFAULT 0,
    status                     TEXT        NOT NULL DEFAULT 'pending'
                               CHECK (status IN ('pending', 'disbursed', 'cancelled')),
    withdrawal_request_id      UUID,
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    settled_at                 TIMESTAMPTZ,
    UNIQUE (tenant_id, carrier_id, period_start, period_end)
);

CREATE INDEX IF NOT EXISTS idx_carrier_settlement_tenant_carrier
    ON payments.carrier_settlement_runs (tenant_id, carrier_id);

CREATE INDEX IF NOT EXISTS idx_carrier_settlement_pending
    ON payments.carrier_settlement_runs (status)
    WHERE status = 'pending';

-- Tag wallet transactions with the settlement run that swept them,
-- and record when they were swept so we can identify unsettled credits.
ALTER TABLE payments.wallet_transactions
    ADD COLUMN IF NOT EXISTS carrier_settlement_id UUID
        REFERENCES payments.carrier_settlement_runs(id),
    ADD COLUMN IF NOT EXISTS settled_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_wallet_txn_unsettled_carrier
    ON payments.wallet_transactions (tenant_id, transaction_type)
    WHERE carrier_settlement_id IS NULL
      AND transaction_type IN ('carrier_margin', 'cod_commission');
