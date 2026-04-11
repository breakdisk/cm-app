CREATE SCHEMA IF NOT EXISTS payments;

-- Invoices issued to merchants for platform usage
CREATE TABLE IF NOT EXISTS payments.invoices (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    merchant_id UUID        NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'issued'
                            CHECK (status IN ('draft','issued','paid','overdue','disputed','cancelled')),
    line_items  JSONB       NOT NULL DEFAULT '[]',
    currency    TEXT        NOT NULL DEFAULT 'PHP',
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    due_at      TIMESTAMPTZ NOT NULL,
    paid_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_invoices_merchant ON payments.invoices(merchant_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status   ON payments.invoices(tenant_id, status);

ALTER TABLE payments.invoices ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.invoices
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- COD collections — one per shipment with COD payment
CREATE TABLE IF NOT EXISTS payments.cod_collections (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    shipment_id  UUID        NOT NULL UNIQUE,
    driver_id    UUID        NOT NULL,
    pod_id       UUID        NOT NULL,
    amount_cents BIGINT      NOT NULL CHECK (amount_cents > 0),
    currency     TEXT        NOT NULL DEFAULT 'PHP',
    status       TEXT        NOT NULL DEFAULT 'collected'
                             CHECK (status IN ('collected','in_batch','remitted','disputed')),
    collected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    remitted_at  TIMESTAMPTZ,
    batch_id     UUID
);

CREATE INDEX IF NOT EXISTS idx_cod_tenant_status ON payments.cod_collections(tenant_id, status);

ALTER TABLE payments.cod_collections ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.cod_collections
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Merchant wallets (one per tenant)
CREATE TABLE IF NOT EXISTS payments.wallets (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL UNIQUE,
    balance_cents BIGINT      NOT NULL DEFAULT 0 CHECK (balance_cents >= 0),
    currency      TEXT        NOT NULL DEFAULT 'PHP',
    version       BIGINT      NOT NULL DEFAULT 0,  -- Optimistic lock version
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE payments.wallets ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.wallets
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Immutable wallet ledger (append-only — never UPDATE or DELETE)
CREATE TABLE IF NOT EXISTS payments.wallet_transactions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id        UUID        NOT NULL REFERENCES payments.wallets(id),
    tenant_id        UUID        NOT NULL,
    transaction_type TEXT        NOT NULL,
    amount_cents     BIGINT      NOT NULL,
    currency         TEXT        NOT NULL DEFAULT 'PHP',
    reference_id     UUID        NOT NULL,
    description      TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wallet_txn_wallet ON payments.wallet_transactions(wallet_id, created_at DESC);

ALTER TABLE payments.wallet_transactions ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.wallet_transactions
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Prevent any UPDATE or DELETE on the ledger table (immutable audit trail)
CREATE OR REPLACE RULE no_update_wallet_transactions AS
    ON UPDATE TO payments.wallet_transactions DO INSTEAD NOTHING;
CREATE OR REPLACE RULE no_delete_wallet_transactions AS
    ON DELETE TO payments.wallet_transactions DO INSTEAD NOTHING;
