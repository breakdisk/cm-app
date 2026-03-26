CREATE SCHEMA IF NOT EXISTS hub_ops;

CREATE TABLE IF NOT EXISTS hub_ops.hubs (
    id             UUID        PRIMARY KEY,
    tenant_id      UUID        NOT NULL,
    name           TEXT        NOT NULL,
    address        TEXT        NOT NULL DEFAULT '',
    lat            FLOAT8      NOT NULL DEFAULT 0,
    lng            FLOAT8      NOT NULL DEFAULT 0,
    capacity       INTEGER     NOT NULL DEFAULT 1000,
    current_load   INTEGER     NOT NULL DEFAULT 0,
    serving_zones  TEXT[]      NOT NULL DEFAULT '{}',
    is_active      BOOLEAN     NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS hub_ops.parcel_inductions (
    id              UUID        PRIMARY KEY,
    hub_id          UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    tenant_id       UUID        NOT NULL,
    shipment_id     UUID        NOT NULL,
    tracking_number TEXT        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'inducted',
    zone            TEXT,
    bay             TEXT,
    inducted_by     UUID,
    inducted_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    sorted_at       TIMESTAMPTZ,
    dispatched_at   TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS hub_parcel_unique
    ON hub_ops.parcel_inductions (hub_id, shipment_id)
    WHERE status NOT IN ('dispatched', 'returned');

CREATE INDEX IF NOT EXISTS hub_inductions_active
    ON hub_ops.parcel_inductions (hub_id, status)
    WHERE status IN ('inducted', 'sorted');

CREATE INDEX IF NOT EXISTS hub_tenant ON hub_ops.hubs (tenant_id);

ALTER TABLE hub_ops.hubs ENABLE ROW LEVEL SECURITY;
ALTER TABLE hub_ops.parcel_inductions ENABLE ROW LEVEL SECURITY;

CREATE POLICY hub_tenant ON hub_ops.hubs USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));
CREATE POLICY induction_tenant ON hub_ops.parcel_inductions USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

CREATE OR REPLACE FUNCTION hub_ops.set_updated_at() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END; $$;

CREATE TRIGGER trg_hubs_updated_at BEFORE UPDATE ON hub_ops.hubs
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();
