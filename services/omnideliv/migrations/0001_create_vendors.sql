-- OmniDeliv product tier. See docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md
CREATE SCHEMA IF NOT EXISTS omnideliv;

-- NAMING: `vendor`, never `merchant`. A LogisticOS Merchant pays the Partner to
-- ship parcels; an OmniDeliv vendor receives money from the Partner for goods.
-- Opposite money flow, different lifecycle. UI copy still says "merchant".
--
-- TENANCY: application-layer, not RLS. Every repository query filters on
-- tenant_id explicitly. See the field-ops plan for why a policy here would
-- imply a database guarantee that does not exist on this platform.

CREATE TABLE IF NOT EXISTS omnideliv.vendors (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL,
    vertical          TEXT        NOT NULL
                                  CHECK (vertical IN ('restaurant','grocery','pharmacy','florist','retail')),
    name              TEXT        NOT NULL,
    address           TEXT        NOT NULL,
    lat               DOUBLE PRECISION NOT NULL,
    lng               DOUBLE PRECISION NOT NULL,
    -- Kitchen/pick time. The Fleet agent sequences stops by this, so a grocery
    -- pick (5 min) is collected before a restaurant main (20 min) and nothing
    -- sits going cold.
    prep_time_minutes INT         NOT NULL DEFAULT 15 CHECK (prep_time_minutes >= 0),
    -- Commission in basis points (250 = 2.50%). Basis points, not a float —
    -- this multiplies money.
    commission_bps    INT         NOT NULL DEFAULT 1500
                                  CHECK (commission_bps BETWEEN 0 AND 10000),
    payout_account    TEXT,
    -- Opening hours as {"mon": [["09:00","21:00"]], ...}. JSONB because the
    -- shape varies per vertical (pharmacies have split shifts, groceries don't).
    hours             JSONB       NOT NULL DEFAULT '{}',
    status            TEXT        NOT NULL DEFAULT 'onboarding'
                                  CHECK (status IN ('onboarding','active','paused','offboarded')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_tenant_vertical
    ON omnideliv.vendors (tenant_id, vertical)
    WHERE status = 'active';

-- Supply lookup: active vendors of a vertical near the customer.
CREATE INDEX IF NOT EXISTS idx_vendor_geo
    ON omnideliv.vendors (tenant_id, lat, lng)
    WHERE status = 'active';
