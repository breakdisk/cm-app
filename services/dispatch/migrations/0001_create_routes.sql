-- Migration: 0001 — Dispatch: Routes and driver assignments

CREATE TABLE IF NOT EXISTS dispatch.routes (
    id                          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id                   UUID        NOT NULL,
    driver_id                   UUID        NOT NULL,
    vehicle_id                  UUID        NOT NULL,
    status                      TEXT        NOT NULL DEFAULT 'planned'
                                            CHECK (status IN ('planned','in_progress','completed','cancelled')),
    total_distance_km           DOUBLE PRECISION,
    estimated_duration_minutes  INTEGER,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at                  TIMESTAMPTZ,
    completed_at                TIMESTAMPTZ,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS dispatch.route_stops (
    id                  UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    route_id            UUID        NOT NULL REFERENCES dispatch.routes(id) ON DELETE CASCADE,
    tenant_id           UUID        NOT NULL,
    shipment_id         UUID        NOT NULL,
    sequence            INTEGER     NOT NULL,
    stop_type           TEXT        NOT NULL CHECK (stop_type IN ('pickup','delivery')),
    address_line1       TEXT        NOT NULL,
    address_city        TEXT        NOT NULL,
    address_province    TEXT        NOT NULL,
    lat                 DOUBLE PRECISION,
    lng                 DOUBLE PRECISION,
    point               GEOGRAPHY(POINT, 4326),
    time_window_start   TIMESTAMPTZ,
    time_window_end     TIMESTAMPTZ,
    estimated_arrival   TIMESTAMPTZ,
    actual_arrival      TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_routes_tenant_driver  ON dispatch.routes (tenant_id, driver_id);
CREATE INDEX IF NOT EXISTS idx_routes_status         ON dispatch.routes (tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_stops_route_sequence  ON dispatch.route_stops (route_id, sequence);
CREATE INDEX IF NOT EXISTS idx_stops_point           ON dispatch.route_stops USING GIST (point);

CREATE OR REPLACE FUNCTION dispatch.sync_stop_point()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.lat IS NOT NULL AND NEW.lng IS NOT NULL THEN
        NEW.point = ST_SetSRID(ST_MakePoint(NEW.lng, NEW.lat), 4326)::geography;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS stop_sync_point ON dispatch.route_stops;
DROP TRIGGER IF EXISTS stop_sync_point ON dispatch.route_stops;
CREATE TRIGGER stop_sync_point
    BEFORE INSERT OR UPDATE ON dispatch.route_stops
    FOR EACH ROW EXECUTE FUNCTION dispatch.sync_stop_point();

ALTER TABLE dispatch.routes      ENABLE ROW LEVEL SECURITY;
ALTER TABLE dispatch.routes      FORCE ROW LEVEL SECURITY;
ALTER TABLE dispatch.route_stops ENABLE ROW LEVEL SECURITY;
ALTER TABLE dispatch.route_stops FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_isolation ON dispatch.routes;
DROP POLICY IF EXISTS tenant_isolation ON dispatch.routes;
CREATE POLICY tenant_isolation ON dispatch.routes      USING (tenant_id = current_setting('app.tenant_id')::uuid);
DROP POLICY IF EXISTS tenant_isolation ON dispatch.route_stops;
DROP POLICY IF EXISTS tenant_isolation ON dispatch.route_stops;
CREATE POLICY tenant_isolation ON dispatch.route_stops USING (tenant_id = current_setting('app.tenant_id')::uuid);
