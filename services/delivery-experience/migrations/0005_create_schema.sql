-- Delivery Experience service schema
CREATE SCHEMA IF NOT EXISTS delivery_experience;

-- Tracking records — one per shipment, updated via Kafka events
CREATE TABLE IF NOT EXISTS delivery_experience.tracking (
    shipment_id          UUID         PRIMARY KEY,
    tenant_id            UUID         NOT NULL,
    tracking_number      TEXT         NOT NULL,
    current_status       TEXT         NOT NULL DEFAULT 'pending',
    status_history       JSONB        NOT NULL DEFAULT '[]',
    origin_address       TEXT         NOT NULL DEFAULT '',
    destination_address  TEXT         NOT NULL DEFAULT '',
    driver_id            UUID,
    driver_name          TEXT,
    driver_phone         TEXT,
    driver_position      JSONB,
    estimated_delivery   TIMESTAMPTZ,
    delivered_at         TIMESTAMPTZ,
    pod_id               UUID,
    recipient_name       TEXT,
    attempt_number       SMALLINT     NOT NULL DEFAULT 0,
    next_attempt_at      TIMESTAMPTZ,
    reschedule_count     INTEGER      NOT NULL DEFAULT 0,
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tracking_number
    ON delivery_experience.tracking (tracking_number);

CREATE INDEX IF NOT EXISTS idx_tracking_tenant
    ON delivery_experience.tracking (tenant_id);

-- Auto-update updated_at
CREATE OR REPLACE FUNCTION delivery_experience.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$;

DO $$ BEGIN
    DROP TRIGGER IF EXISTS trg_tracking_updated_at ON delivery_experience.tracking;
DROP TRIGGER IF EXISTS trg_tracking_updated_at ON delivery_experience.tracking;
CREATE TRIGGER trg_tracking_updated_at
        BEFORE UPDATE ON delivery_experience.tracking
        FOR EACH ROW EXECUTE FUNCTION delivery_experience.set_updated_at();
EXCEPTION WHEN duplicate_object THEN NULL; END; $$;

-- Row-level security for tenant isolation
ALTER TABLE delivery_experience.tracking ENABLE ROW LEVEL SECURITY;
ALTER TABLE delivery_experience.tracking FORCE ROW LEVEL SECURITY;

DO $$ BEGIN
    DROP POLICY IF EXISTS tenant_isolation ON delivery_experience.tracking;
DROP POLICY IF EXISTS tenant_isolation ON delivery_experience.tracking;
CREATE POLICY tenant_isolation ON delivery_experience.tracking
        USING (tenant_id = current_setting('app.tenant_id', true)::uuid);
EXCEPTION WHEN duplicate_object THEN NULL; END; $$;
