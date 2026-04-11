-- CDP: Customer Data Platform schema
-- Multi-tenant via RLS on tenant_id.
-- address_history and recent_events stored as JSONB for flexible schema evolution.

CREATE SCHEMA IF NOT EXISTS cdp;

-- ─── Customer Profiles ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS cdp.customer_profiles (
    id                          UUID            PRIMARY KEY,
    tenant_id                   UUID            NOT NULL,

    -- Joins back to order-intake / identity customer records
    external_customer_id        UUID            NOT NULL,

    -- Identity (may be enriched from multiple sources over time)
    name                        TEXT,
    email                       TEXT,
    phone                       TEXT,

    -- Delivery counters (denormalised for fast analytics queries)
    total_shipments             INTEGER         NOT NULL DEFAULT 0,
    successful_deliveries       INTEGER         NOT NULL DEFAULT 0,
    failed_deliveries           INTEGER         NOT NULL DEFAULT 0,
    first_shipment_at           TIMESTAMPTZ,
    last_shipment_at            TIMESTAMPTZ,

    -- COD aggregate
    total_cod_collected_cents   BIGINT          NOT NULL DEFAULT 0,

    -- Address intelligence — JSONB array of {address, use_count, last_used}
    address_history             JSONB           NOT NULL DEFAULT '[]'::jsonb,

    -- Behavioral timeline — last 90 events, JSONB array
    recent_events               JSONB           NOT NULL DEFAULT '[]'::jsonb,

    -- Computed scores (updated on each event)
    clv_score                   REAL            NOT NULL DEFAULT 0.0,
    engagement_score            REAL            NOT NULL DEFAULT 0.0,

    created_at                  TIMESTAMPTZ     NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ     NOT NULL DEFAULT now()
);

-- One profile per external customer per tenant
CREATE UNIQUE INDEX IF NOT EXISTS cdp_profiles_tenant_external
    ON cdp.customer_profiles (tenant_id, external_customer_id);

-- Email lookup (nullable, so partial index)
CREATE INDEX IF NOT EXISTS cdp_profiles_email
    ON cdp.customer_profiles (tenant_id, email)
    WHERE email IS NOT NULL;

-- CLV leaderboard queries
CREATE INDEX IF NOT EXISTS cdp_profiles_clv
    ON cdp.customer_profiles (tenant_id, clv_score DESC);

-- Recent activity
CREATE INDEX IF NOT EXISTS cdp_profiles_last_shipment
    ON cdp.customer_profiles (tenant_id, last_shipment_at DESC NULLS LAST);

-- ─── Row-Level Security ───────────────────────────────────────────────────────

ALTER TABLE cdp.customer_profiles ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS cdp_profiles_tenant_isolation ON cdp.customer_profiles;
DROP POLICY IF EXISTS cdp_profiles_tenant_isolation ON cdp.customer_profiles;
CREATE POLICY cdp_profiles_tenant_isolation ON cdp.customer_profiles
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

-- ─── updated_at trigger ───────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION cdp.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_customer_profiles_updated_at ON cdp.customer_profiles;
DROP TRIGGER IF EXISTS trg_customer_profiles_updated_at ON cdp.customer_profiles;
CREATE TRIGGER trg_customer_profiles_updated_at
    BEFORE UPDATE ON cdp.customer_profiles
    FOR EACH ROW EXECUTE FUNCTION cdp.set_updated_at();
