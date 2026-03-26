-- Fleet: Telematics and fuel records
CREATE TABLE IF NOT EXISTS fleet.telematics_events (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    vehicle_id      UUID        NOT NULL REFERENCES fleet.vehicles(id),
    driver_id       UUID,
    lat             FLOAT8      NOT NULL,
    lng             FLOAT8      NOT NULL,
    speed_kmh       FLOAT4      NOT NULL DEFAULT 0,
    heading_deg     FLOAT4,
    ignition        BOOLEAN     NOT NULL DEFAULT false,
    odometer_km     INTEGER,
    fuel_level_pct  FLOAT4,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, recorded_at)
);

CREATE INDEX IF NOT EXISTS idx_telematics_vehicle_time ON fleet.telematics_events(vehicle_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_telematics_tenant_time  ON fleet.telematics_events(tenant_id, recorded_at DESC);
ALTER TABLE fleet.telematics_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON fleet.telematics_events USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS fleet.fuel_records (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    vehicle_id      UUID        NOT NULL REFERENCES fleet.vehicles(id),
    driver_id       UUID,
    liters          FLOAT4      NOT NULL,
    cost_cents      INTEGER     NOT NULL,
    odometer_km     INTEGER     NOT NULL,
    station         TEXT,
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_fuel_vehicle ON fleet.fuel_records(tenant_id, vehicle_id, recorded_at DESC);
ALTER TABLE fleet.fuel_records ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON fleet.fuel_records USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
