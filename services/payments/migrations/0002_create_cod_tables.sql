-- Migration: 0002 — Payments: COD reconciliation, batch management, and multi-owner wallets
-- Extends the payments schema with full COD lifecycle tracking (collection → batch →
-- remittance → reconciliation) and a generalised wallet system supporting merchant,
-- driver, and carrier owner types.
--
-- COD Flow:
--   1. Driver collects cash at door → payments.cod_records (status: pending → collected)
--   2. Driver submits end-of-day batch → payments.cod_batches (status: open → submitted)
--   3. Hub manager verifies batch → cod_batches (submitted → verified)
--   4. Finance reconciles and transfers to merchant wallet → cod_batches (verified → reconciled)
--   5. cod_records marked reconciled; wallet_transactions created for both parties

-- ─── COD Records ──────────────────────────────────────────────────────────────

-- payments.cod_records is the authoritative record of a single COD collection
-- event. One record exists per shipment that requires COD payment.
-- Status transitions (enforced in application layer):
--   pending → collected (driver confirms cash received at door)
--   collected → remitted (included in a submitted cod_batch)
--   remitted → reconciled (finance confirms batch reconciliation)
CREATE TABLE IF NOT EXISTS payments.cod_records (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    shipment_id     UUID        NOT NULL,                 -- FK to order-intake schema (cross-service reference — no DB FK)
    driver_id       UUID        NOT NULL,
    amount_cents    BIGINT      NOT NULL CHECK (amount_cents > 0),
    currency        TEXT        NOT NULL DEFAULT 'PHP',

    status          TEXT        NOT NULL DEFAULT 'pending'
                                CHECK (status IN (
                                    'pending',      -- order created, delivery not yet attempted
                                    'collected',    -- driver collected cash at door
                                    'remitted',     -- included in a submitted cod_batch
                                    'reconciled'    -- finance confirmed reconciliation
                                )),

    -- Timestamps for each status transition
    collected_at    TIMESTAMPTZ,
    remitted_at     TIMESTAMPTZ,
    reconciled_at   TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Each shipment can only have one COD record.
    UNIQUE (shipment_id)
);

CREATE INDEX IF NOT EXISTS idx_cod_records_tenant_status
    ON payments.cod_records (tenant_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_cod_records_driver
    ON payments.cod_records (tenant_id, driver_id, status);

CREATE INDEX IF NOT EXISTS idx_cod_records_shipment
    ON payments.cod_records (shipment_id);

ALTER TABLE payments.cod_records ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS cod_records_tenant_isolation ON payments.cod_records;
CREATE POLICY cod_records_tenant_isolation ON payments.cod_records
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─── COD Batches ──────────────────────────────────────────────────────────────

-- payments.cod_batches represents a driver's end-of-day or per-route cash submission.
-- A batch groups multiple cod_records for a single driver on a single date.
-- Batches are verified by hub managers and reconciled by finance staff.
CREATE TABLE IF NOT EXISTS payments.cod_batches (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    driver_id           UUID        NOT NULL,
    batch_date          DATE        NOT NULL,

    total_amount_cents  BIGINT      NOT NULL DEFAULT 0 CHECK (total_amount_cents >= 0),
    parcel_count        INTEGER     NOT NULL DEFAULT 0 CHECK (parcel_count >= 0),

    status              TEXT        NOT NULL DEFAULT 'open'
                                    CHECK (status IN (
                                        'open',         -- batch created, can still add items
                                        'submitted',    -- driver submitted for hub verification
                                        'verified',     -- hub manager confirmed counts
                                        'reconciled'    -- finance completed reconciliation
                                    )),

    -- User who verified this batch at the hub.
    verified_by_user_id UUID,

    -- Optional notes from the hub manager (e.g., "₱50 discrepancy — recount confirmed").
    verification_notes  TEXT,

    submitted_at        TIMESTAMPTZ,
    verified_at         TIMESTAMPTZ,
    reconciled_at       TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One batch per driver per date per tenant (enforced here; open batches created lazily).
    UNIQUE (tenant_id, driver_id, batch_date)
);

CREATE INDEX IF NOT EXISTS idx_cod_batches_tenant_status
    ON payments.cod_batches (tenant_id, status, batch_date DESC);

CREATE INDEX IF NOT EXISTS idx_cod_batches_driver
    ON payments.cod_batches (tenant_id, driver_id, batch_date DESC);

ALTER TABLE payments.cod_batches ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS cod_batches_tenant_isolation ON payments.cod_batches;
CREATE POLICY cod_batches_tenant_isolation ON payments.cod_batches
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─── COD Batch Items ──────────────────────────────────────────────────────────

-- payments.cod_batch_items is the join table linking cod_records to cod_batches.
-- A cod_record may only belong to one batch (enforced by the unique constraint
-- on cod_records.shipment_id + the application-level constraint that a record
-- can only be remitted once).
CREATE TABLE IF NOT EXISTS payments.cod_batch_items (
    batch_id        UUID    NOT NULL REFERENCES payments.cod_batches(id),
    cod_record_id   UUID    NOT NULL REFERENCES payments.cod_records(id),

    PRIMARY KEY (batch_id, cod_record_id)
);

CREATE INDEX IF NOT EXISTS idx_cod_batch_items_batch
    ON payments.cod_batch_items (batch_id);

CREATE INDEX IF NOT EXISTS idx_cod_batch_items_record
    ON payments.cod_batch_items (cod_record_id);

-- ─── Wallets ──────────────────────────────────────────────────────────────────

-- payments.wallets extends the existing single-tenant wallet model to support
-- multiple wallet owners: merchants, drivers, and carrier partners.
-- Each (tenant_id, owner_type, owner_id) combination has at most one wallet.
--
-- The existing payments.wallets table from migration 0001 was scoped to one
-- wallet per tenant. This migration drops the old UNIQUE constraint and replaces
-- it with the multi-owner model. If 0001 created a tenant-scoped wallet, it is
-- retroactively treated as owner_type='merchant' with owner_id = tenant_id.
--
-- IMPORTANT: If the payments.wallets table already exists from migration 0001,
-- this migration adds the new columns and constraint — it does not recreate the table.

-- Add owner columns if they don't already exist (idempotent ALTER TABLE pattern).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'payments' AND table_name = 'wallets' AND column_name = 'owner_type'
    ) THEN
        ALTER TABLE payments.wallets
            ADD COLUMN owner_type   TEXT    NOT NULL DEFAULT 'merchant'
                                            CHECK (owner_type IN ('merchant', 'driver', 'carrier')),
            ADD COLUMN owner_id     UUID    NOT NULL DEFAULT gen_random_uuid(),
            ADD COLUMN reserved_cents BIGINT NOT NULL DEFAULT 0
                                            CHECK (reserved_cents >= 0);

        -- Drop the old single-tenant UNIQUE constraint if it exists
        ALTER TABLE payments.wallets DROP CONSTRAINT IF EXISTS wallets_tenant_id_key;

        -- Add the multi-owner unique constraint
        ALTER TABLE payments.wallets
            ADD CONSTRAINT wallets_tenant_owner_unique
            UNIQUE (tenant_id, owner_type, owner_id);
    END IF;
END;
$$;

-- If the wallets table did not exist yet (fresh install), create it in full:
CREATE TABLE IF NOT EXISTS payments.wallets (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,

    -- Owner classification and identifier.
    -- merchant: owner_id = merchant_id (from order-intake service)
    -- driver:   owner_id = driver_id
    -- carrier:  owner_id = carrier_id (from carrier-management service)
    owner_type      TEXT        NOT NULL
                                CHECK (owner_type IN ('merchant', 'driver', 'carrier')),
    owner_id        UUID        NOT NULL,

    -- Available balance for withdrawals and payouts.
    balance_cents   BIGINT      NOT NULL DEFAULT 0 CHECK (balance_cents >= 0),

    -- Balance reserved for in-flight transactions (e.g., pending COD remittances).
    -- reserved_cents is always <= balance_cents.
    reserved_cents  BIGINT      NOT NULL DEFAULT 0 CHECK (reserved_cents >= 0),

    currency        TEXT        NOT NULL DEFAULT 'PHP',

    -- Optimistic concurrency lock. Incremented on every balance update.
    version         BIGINT      NOT NULL DEFAULT 0,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (tenant_id, owner_type, owner_id)
);

CREATE INDEX IF NOT EXISTS idx_wallets_tenant_owner
    ON payments.wallets (tenant_id, owner_type, owner_id);

-- Check that reserved never exceeds balance (deferred constraint — evaluated at commit).
ALTER TABLE payments.wallets
    DROP CONSTRAINT IF EXISTS wallets_reserved_le_balance;
ALTER TABLE payments.wallets
    ADD CONSTRAINT wallets_reserved_le_balance
    CHECK (reserved_cents <= balance_cents)
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE payments.wallets ENABLE ROW LEVEL SECURITY;
ALTER TABLE payments.wallets FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS wallets_tenant_isolation ON payments.wallets;
CREATE POLICY wallets_tenant_isolation ON payments.wallets
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─── Wallet Transactions ──────────────────────────────────────────────────────

-- Extend the existing wallet_transactions table with additional reference fields
-- if it was created by migration 0001 (which had a narrower schema).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'payments'
          AND table_name   = 'wallet_transactions'
          AND column_name  = 'reference_type'
    ) THEN
        ALTER TABLE payments.wallet_transactions
            ADD COLUMN reference_type TEXT,     -- 'cod_batch', 'invoice', 'payout', 'adjustment'
            ADD COLUMN note          TEXT;
    END IF;
END;
$$;

-- Create wallet_transactions from scratch if the table doesn't exist.
CREATE TABLE IF NOT EXISTS payments.wallet_transactions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id       UUID        NOT NULL REFERENCES payments.wallets(id),
    tenant_id       UUID        NOT NULL,

    -- Transaction type determines the direction and business meaning of the entry.
    -- credit types: 'cod_remittance', 'topup', 'refund_credit', 'adjustment_credit'
    -- debit  types: 'payout', 'fee', 'refund_debit', 'adjustment_debit', 'reserve'
    -- neutral:      'reserve_release'
    type            TEXT        NOT NULL,

    -- Positive value = credit to wallet; negative value = debit from wallet.
    -- Always stored as a signed integer in minor currency units.
    amount_cents    BIGINT      NOT NULL,

    -- Reference to the upstream business object that caused this transaction.
    reference_id    UUID        NOT NULL,      -- e.g., cod_batch.id, invoice.id, payout_request.id
    reference_type  TEXT        NOT NULL,      -- e.g., 'cod_batch', 'invoice', 'payout'

    -- Human-readable description for statements and dispute resolution.
    note            TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wallet_txn_wallet
    ON payments.wallet_transactions (wallet_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_wallet_txn_reference
    ON payments.wallet_transactions (reference_id, reference_type);

CREATE INDEX IF NOT EXISTS idx_wallet_txn_tenant
    ON payments.wallet_transactions (tenant_id, created_at DESC);

ALTER TABLE payments.wallet_transactions ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS wallet_txn_tenant_isolation ON payments.wallet_transactions;
CREATE POLICY wallet_txn_tenant_isolation ON payments.wallet_transactions
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Immutable ledger — no updates or deletes allowed.
CREATE OR REPLACE RULE no_update_wallet_transactions AS
    ON UPDATE TO payments.wallet_transactions DO INSTEAD NOTHING;
CREATE OR REPLACE RULE no_delete_wallet_transactions AS
    ON DELETE TO payments.wallet_transactions DO INSTEAD NOTHING;

-- ─── Triggers: updated_at maintenance ─────────────────────────────────────────

CREATE OR REPLACE FUNCTION payments.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_cod_records_updated_at ON payments.cod_records;
CREATE TRIGGER trg_cod_records_updated_at
    BEFORE UPDATE ON payments.cod_records
    FOR EACH ROW EXECUTE FUNCTION payments.set_updated_at();

DROP TRIGGER IF EXISTS trg_cod_batches_updated_at ON payments.cod_batches;
CREATE TRIGGER trg_cod_batches_updated_at
    BEFORE UPDATE ON payments.cod_batches
    FOR EACH ROW EXECUTE FUNCTION payments.set_updated_at();

DROP TRIGGER IF EXISTS trg_wallets_updated_at ON payments.wallets;
CREATE TRIGGER trg_wallets_updated_at
    BEFORE UPDATE ON payments.wallets
    FOR EACH ROW EXECUTE FUNCTION payments.set_updated_at();

-- ─── Helper: open or get current COD batch for a driver ───────────────────────

-- payments.get_or_create_cod_batch(p_tenant_id, p_driver_id, p_date)
-- Returns the existing open batch for this driver+date, or creates one.
-- Called by the COD collection API when a driver reports a cash collection.
CREATE OR REPLACE FUNCTION payments.get_or_create_cod_batch(
    p_tenant_id UUID,
    p_driver_id UUID,
    p_date      DATE DEFAULT CURRENT_DATE
)
RETURNS payments.cod_batches
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_batch payments.cod_batches;
BEGIN
    -- Try to fetch an existing open batch
    SELECT * INTO v_batch
    FROM payments.cod_batches
    WHERE tenant_id  = p_tenant_id
      AND driver_id  = p_driver_id
      AND batch_date = p_date
      AND status     = 'open';

    -- Create one if it doesn't exist
    IF NOT FOUND THEN
        INSERT INTO payments.cod_batches (tenant_id, driver_id, batch_date)
        VALUES (p_tenant_id, p_driver_id, p_date)
        ON CONFLICT (tenant_id, driver_id, batch_date) DO NOTHING
        RETURNING * INTO v_batch;

        -- Handle race condition: another session may have created the batch
        IF NOT FOUND THEN
            SELECT * INTO v_batch
            FROM payments.cod_batches
            WHERE tenant_id  = p_tenant_id
              AND driver_id  = p_driver_id
              AND batch_date = p_date;
        END IF;
    END IF;

    RETURN v_batch;
END;
$$;

COMMENT ON FUNCTION payments.get_or_create_cod_batch IS
    'Returns the open COD batch for a driver on a given date, creating one if needed. '
    'Safe for concurrent calls — uses ON CONFLICT to handle races.';
