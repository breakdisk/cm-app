CREATE SCHEMA IF NOT EXISTS fleet;

CREATE TABLE IF NOT EXISTS fleet.vehicles (
    id                    UUID            PRIMARY KEY,
    tenant_id             UUID            NOT NULL,
    plate_number          TEXT            NOT NULL,
    vehicle_type          TEXT            NOT NULL DEFAULT 'motorcycle',
    make                  TEXT            NOT NULL DEFAULT '',
    model                 TEXT            NOT NULL DEFAULT '',
    year                  SMALLINT        NOT NULL,
    color                 TEXT            NOT NULL DEFAULT '',
    status                TEXT            NOT NULL DEFAULT 'active',
    assigned_driver_id    UUID,
    odometer_km           INTEGER         NOT NULL DEFAULT 0,
    maintenance_history   JSONB           NOT NULL DEFAULT '[]'::jsonb,
    next_maintenance_due  DATE,
    created_at            TIMESTAMPTZ     NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ     NOT NULL DEFAULT now()
);

-- Plate number must be unique per tenant
CREATE UNIQUE INDEX IF NOT EXISTS fleet_plate_tenant
    ON fleet.vehicles (tenant_id, plate_number);

-- Driver assignment index
CREATE UNIQUE INDEX IF NOT EXISTS fleet_driver_assignment
    ON fleet.vehicles (tenant_id, assigned_driver_id)
    WHERE assigned_driver_id IS NOT NULL AND status = 'active';

-- Maintenance due filter
CREATE INDEX IF NOT EXISTS fleet_maintenance_due
    ON fleet.vehicles (tenant_id, next_maintenance_due)
    WHERE next_maintenance_due IS NOT NULL;

ALTER TABLE fleet.vehicles ENABLE ROW LEVEL SECURITY;

CREATE POLICY fleet_tenant_isolation ON fleet.vehicles
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

CREATE OR REPLACE FUNCTION fleet.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$;

CREATE TRIGGER trg_fleet_vehicles_updated_at
    BEFORE UPDATE ON fleet.vehicles
    FOR EACH ROW EXECUTE FUNCTION fleet.set_updated_at();
