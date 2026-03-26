-- Driver location time-series — uses TimescaleDB hypertable for efficient range queries.
-- Partitioned by recorded_at (daily chunks). Retention policy: 90 days.
CREATE TABLE IF NOT EXISTS driver_ops.driver_locations (
    driver_id    UUID             NOT NULL,
    tenant_id    UUID             NOT NULL,
    lat          DOUBLE PRECISION NOT NULL,
    lng          DOUBLE PRECISION NOT NULL,
    accuracy_m   REAL,
    speed_kmh    REAL,
    heading      REAL,
    battery_pct  SMALLINT,
    recorded_at  TIMESTAMPTZ      NOT NULL,
    received_at  TIMESTAMPTZ      NOT NULL DEFAULT NOW()
);

-- Convert to TimescaleDB hypertable (partitioned by time, 1-day chunks)
SELECT create_hypertable(
    'driver_ops.driver_locations',
    'recorded_at',
    chunk_time_interval => INTERVAL '1 day',
    if_not_exists => TRUE
);

-- Composite index for per-driver time-ordered queries
CREATE INDEX IF NOT EXISTS idx_driver_locations_driver_time
    ON driver_ops.driver_locations (driver_id, recorded_at DESC);

-- PostGIS index for spatial queries (dispatch proximity search uses this)
-- This is the authoritative location table; drivers table caches last-known position.
CREATE INDEX IF NOT EXISTS idx_driver_locations_spatial
    ON driver_ops.driver_locations
    USING GIST (ST_SetSRID(ST_MakePoint(lng, lat), 4326)::geography);

-- Compression policy: compress chunks older than 7 days (significant storage saving)
SELECT add_compression_policy('driver_ops.driver_locations', INTERVAL '7 days', if_not_exists => TRUE);

-- Retention policy: automatically drop data older than 90 days
SELECT add_retention_policy('driver_ops.driver_locations', INTERVAL '90 days', if_not_exists => TRUE);

-- This is the view dispatch uses for the live driver ping (replaces the ad-hoc subquery)
CREATE OR REPLACE VIEW driver_ops.driver_latest_locations AS
SELECT DISTINCT ON (driver_id)
    driver_id,
    tenant_id,
    lat,
    lng,
    speed_kmh,
    heading,
    accuracy_m,
    battery_pct,
    recorded_at
FROM driver_ops.driver_locations
ORDER BY driver_id, recorded_at DESC;
