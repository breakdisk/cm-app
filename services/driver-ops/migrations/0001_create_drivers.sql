-- Driver profiles — one per field agent, linked to identity.users via user_id.
CREATE SCHEMA IF NOT EXISTS driver_ops;

CREATE TABLE IF NOT EXISTS driver_ops.drivers (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    user_id          UUID        NOT NULL UNIQUE,   -- FK to identity.users.id (cross-schema)
    first_name       TEXT        NOT NULL,
    last_name        TEXT        NOT NULL,
    phone            TEXT        NOT NULL,
    status           TEXT        NOT NULL DEFAULT 'offline'
                                 CHECK (status IN ('offline','available','en_route','delivering','returning','on_break')),
    -- Denormalized last-known position for fast nearest-driver queries in dispatch
    lat              DOUBLE PRECISION,
    lng              DOUBLE PRECISION,
    last_location_at TIMESTAMPTZ,
    vehicle_id       UUID,                          -- FK to fleet.vehicles (future)
    active_route_id  UUID,                          -- FK to dispatch.routes
    is_active        BOOLEAN     NOT NULL DEFAULT true,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_drivers_tenant_id ON driver_ops.drivers(tenant_id);
CREATE INDEX IF NOT EXISTS idx_drivers_status    ON driver_ops.drivers(tenant_id, status)
    WHERE is_active = true;

-- PostGIS spatial index for proximity queries (used by dispatch's find_available_near)
CREATE INDEX IF NOT EXISTS idx_drivers_location
    ON driver_ops.drivers USING GIST (
        geography(ST_SetSRID(ST_MakePoint(lng, lat), 4326))
    )
    WHERE lat IS NOT NULL AND lng IS NOT NULL;

ALTER TABLE driver_ops.drivers ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON driver_ops.drivers;
DROP POLICY IF EXISTS tenant_isolation ON driver_ops.drivers;
CREATE POLICY tenant_isolation ON driver_ops.drivers
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE OR REPLACE FUNCTION driver_ops.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_drivers_updated_at ON driver_ops.drivers;
DROP TRIGGER IF EXISTS trg_drivers_updated_at ON driver_ops.drivers;
CREATE TRIGGER trg_drivers_updated_at
    BEFORE UPDATE ON driver_ops.drivers
    FOR EACH ROW EXECUTE FUNCTION driver_ops.set_updated_at();
