-- Vendor payouts, modelled on the existing DriverLedger: an append-only entry
-- log with a denormalised balance. Entries are never updated or deleted —
-- a correction is a new compensating entry, so the history stays auditable.

CREATE TABLE IF NOT EXISTS omnideliv.vendor_ledgers (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    vendor_id     UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    -- Payout period, e.g. '2026-W32'. One open ledger per vendor per period.
    period        TEXT        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open','closed','settled')),
    balance_cents BIGINT      NOT NULL DEFAULT 0,
    -- Optimistic lock. Two concurrent pickups crediting the same vendor must
    -- not lose an entry to a last-write-wins race.
    version       BIGINT      NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_vendor_ledger_period
    ON omnideliv.vendor_ledgers (tenant_id, vendor_id, period);

CREATE TABLE IF NOT EXISTS omnideliv.vendor_ledger_entries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id    UUID        NOT NULL REFERENCES omnideliv.vendor_ledgers(id),
    kind         TEXT        NOT NULL
                             CHECK (kind IN ('goods_credit','commission_debit','adjustment','payout')),
    -- Signed: credits positive, debits and payouts negative, so the balance is
    -- always a plain SUM and cannot disagree with the log.
    amount_cents BIGINT      NOT NULL,
    order_id     UUID,
    leg_id       UUID,
    reference    TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_entry_ledger
    ON omnideliv.vendor_ledger_entries (ledger_id, created_at);

-- Append-only, stated. NOTE: services connect as the schema owner, and
-- PostgreSQL does not apply a REVOKE FROM PUBLIC to the owner — so this does
-- not stop the owning role today. It is a correct statement of intent that
-- starts enforcing the moment the service runs as a non-owner, which is where
-- the RLS follow-up is heading. Append-only is enforced in the meantime by the
-- entity having no method that edits an existing entry, and by the test that
-- asserts a correction grows the log.
REVOKE UPDATE, DELETE ON omnideliv.vendor_ledger_entries FROM PUBLIC;
