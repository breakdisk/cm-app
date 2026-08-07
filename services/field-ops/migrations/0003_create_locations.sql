-- GPS breadcrumbs. High write volume, read almost exclusively as "latest per
-- courier" plus occasional history windows for an audit or a dispute.
--
-- device_timestamp vs recorded_at: per the CLAUDE.md dual-timestamp contract,
-- device_timestamp is the hardware clock at the physical moment of capture and
-- is the primary basis for any SLA or transit-velocity maths. recorded_at is
-- backend receipt time and is only a fallback for server-generated points.

CREATE TABLE IF NOT EXISTS field_ops.courier_locations (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    courier_id       UUID        NOT NULL,
    lat              DOUBLE PRECISION NOT NULL,
    lng              DOUBLE PRECISION NOT NULL,
    accuracy_m       REAL,
    speed_kph        REAL,
    heading_deg      REAL,
    device_timestamp TIMESTAMPTZ,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, recorded_at)
);

-- The dominant read: most recent fix for one courier.
CREATE INDEX IF NOT EXISTS idx_courier_location_latest
    ON field_ops.courier_locations (courier_id, recorded_at DESC);

CREATE INDEX IF NOT EXISTS idx_courier_location_tenant
    ON field_ops.courier_locations (tenant_id, recorded_at DESC);

-- THE proximity index (ADR-0015). Supply lookup is ST_DWithin against a
-- geography; neither btree above can serve it, and without this every "who is
-- near this pickup" scans the table. driver_ops has the same index on
-- driver_locations — carrying it forward is what keeps convergence a
-- repository swap.
CREATE INDEX IF NOT EXISTS idx_courier_location_spatial
    ON field_ops.courier_locations
    USING GIST (geography(ST_SetSRID(ST_MakePoint(lng, lat), 4326)));

-- One definition of "where is this courier now". Dispatch reads the view, never
-- the raw table, so the latest-fix rule lives in exactly one place. driver_ops
-- arrived here the hard way: this view replaced an ad-hoc subquery.
CREATE OR REPLACE VIEW field_ops.courier_latest_locations AS
SELECT DISTINCT ON (courier_id)
    courier_id,
    tenant_id,
    lat,
    lng,
    speed_kph,
    heading_deg,
    accuracy_m,
    device_timestamp,
    recorded_at
FROM field_ops.courier_locations
ORDER BY courier_id, recorded_at DESC;

-- Hypertable conversion, guarded. The EXCEPTION handler is the whole point: on
-- a database without TimescaleDB this degrades to a plain table instead of
-- failing the migration. That matters more here than usual — a migration that
-- cannot apply pins the service to its last-good image, silently, which is how
-- engagement sat seven weeks behind master.
DO $$ BEGIN
    PERFORM create_hypertable(
        'field_ops.courier_locations',
        'recorded_at',
        chunk_time_interval => INTERVAL '1 day',
        if_not_exists => TRUE
    );
EXCEPTION WHEN undefined_function THEN
    NULL;
END $$;

DO $$ BEGIN
    PERFORM add_compression_policy('field_ops.courier_locations', INTERVAL '7 days', if_not_exists => TRUE);
EXCEPTION WHEN undefined_function THEN NULL;
END $$;

DO $$ BEGIN
    PERFORM add_retention_policy('field_ops.courier_locations', INTERVAL '90 days', if_not_exists => TRUE);
EXCEPTION WHEN undefined_function THEN NULL;
END $$;
