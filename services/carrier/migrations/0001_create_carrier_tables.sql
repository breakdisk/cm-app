CREATE SCHEMA IF NOT EXISTS carrier;

CREATE TABLE IF NOT EXISTS carrier.carriers (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    name              TEXT        NOT NULL,
    code              TEXT        NOT NULL,
    contact_email     TEXT        NOT NULL,
    contact_phone     TEXT,
    api_endpoint      TEXT,
    api_key_hash      TEXT,
    status            TEXT        NOT NULL DEFAULT 'pending_verification',
    sla               JSONB       NOT NULL DEFAULT '{}'::jsonb,
    rate_cards        JSONB       NOT NULL DEFAULT '[]'::jsonb,
    total_shipments   BIGINT      NOT NULL DEFAULT 0,
    on_time_count     BIGINT      NOT NULL DEFAULT 0,
    failed_count      BIGINT      NOT NULL DEFAULT 0,
    performance_grade TEXT        NOT NULL DEFAULT 'good',
    onboarded_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS carrier_code_tenant ON carrier.carriers (tenant_id, code);
CREATE INDEX IF NOT EXISTS carrier_active ON carrier.carriers (tenant_id, status);

ALTER TABLE carrier.carriers ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS carrier_tenant_isolation ON carrier.carriers;
DROP POLICY IF EXISTS carrier_tenant_isolation ON carrier.carriers;
CREATE POLICY carrier_tenant_isolation ON carrier.carriers
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

CREATE OR REPLACE FUNCTION carrier.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$;

DROP TRIGGER IF EXISTS trg_carriers_updated_at ON carrier.carriers;
DROP TRIGGER IF EXISTS trg_carriers_updated_at ON carrier.carriers;
CREATE TRIGGER trg_carriers_updated_at
    BEFORE UPDATE ON carrier.carriers
    FOR EACH ROW EXECUTE FUNCTION carrier.set_updated_at();
