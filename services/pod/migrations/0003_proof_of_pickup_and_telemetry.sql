-- ── Proof of Pickup ──────────────────────────────────────────────────────────────
-- Chain-of-custody-open event recorded when a driver collects a parcel from the
-- merchant or hub. The POP bookend mirrors the POD structure: both are immutable
-- evidence bundles; POP opens custody, POD closes it.
--
-- Track B (Standard Parcel): `actual_weight_g` vs `declared_weight_g` drives the
-- 5% overage tolerance check in the billing strategy. Both weights are recorded here
-- at pickup so billing does not need to re-query the driver app at invoice time.

CREATE TABLE IF NOT EXISTS pod.proofs_of_pickup (
    id                      UUID            PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               UUID            NOT NULL,
    shipment_id             UUID            NOT NULL,
    task_id                 UUID            NOT NULL,
    driver_id               UUID            NOT NULL,
    status                  TEXT            NOT NULL DEFAULT 'draft'
                                            CHECK (status IN ('draft', 'submitted')),

    -- GPS at moment of pickup
    capture_lat             DOUBLE PRECISION NOT NULL,
    capture_lng             DOUBLE PRECISION NOT NULL,

    -- Geofence / OUT_OF_BOUNDS_HANDOVER flags
    geofence_verified       BOOLEAN         NOT NULL DEFAULT false,
    -- Soft audit flag: set when driver is >50m from pickup address edge (non-blocking).
    out_of_bounds_handover  BOOLEAN         NOT NULL DEFAULT false,

    -- Photo evidence (optional but recommended for Track A Balikbayan boxes)
    photo_s3_key            TEXT,
    photo_size_bytes        BIGINT,

    -- Weight capture for Track B overage billing
    actual_weight_g         BIGINT,
    declared_weight_g       BIGINT,

    -- Barcode scan confirmation
    barcode_scanned         BOOLEAN         NOT NULL DEFAULT false,
    scanned_barcode         TEXT,

    -- Dual timestamp discipline:
    --   device_timestamp = hardware clock at scan (from driver device)
    --   captured_at      = backend receipt clock (server_timestamp)
    -- Use device_timestamp for SLA; compare with captured_at to detect clock drift > 5 min.
    device_timestamp        TIMESTAMPTZ,
    captured_at             TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    created_at              TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

-- One submitted POP per shipment
CREATE UNIQUE INDEX IF NOT EXISTS uq_pop_shipment_submitted
    ON pod.proofs_of_pickup (shipment_id)
    WHERE status = 'submitted';

CREATE INDEX IF NOT EXISTS idx_pop_tenant   ON pod.proofs_of_pickup (tenant_id);
CREATE INDEX IF NOT EXISTS idx_pop_driver   ON pod.proofs_of_pickup (driver_id, captured_at DESC);
CREATE INDEX IF NOT EXISTS idx_pop_task     ON pod.proofs_of_pickup (task_id);

ALTER TABLE pod.proofs_of_pickup ENABLE ROW LEVEL SECURITY;
CREATE POLICY pop_tenant_isolation ON pod.proofs_of_pickup
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);


-- ── Shipment Telemetry Logs ───────────────────────────────────────────────────
-- Append-only time-series table for all shipment lifecycle milestone events:
-- pickup scans, delivery attempts, POD/POP submissions, geofence exceptions, etc.
--
-- Design rules:
--   1. NEVER UPDATE rows — insert new rows for corrections (event_type = 'correction').
--   2. `device_timestamp` is the canonical event time for SLA calculations.
--   3. `server_timestamp` is the backend receipt clock.
--   4. > 5 min divergence between the two = clock drift audit flag (check metadata.drift_seconds).
--   5. `event_time` = device_timestamp, used as the TimescaleDB partition key.
--
-- Converted to a TimescaleDB hypertable partitioned by `event_time` in 1-month chunks.
-- If TimescaleDB is not installed (plain PostgreSQL), the CREATE TABLE still works;
-- the SELECT create_hypertable call is a no-op (returns an error that the migration
-- runner should tolerate with `IF EXISTS` semantics or by wrapping in a DO block).

CREATE TABLE IF NOT EXISTS pod.shipment_telemetry_logs (
    id               BIGSERIAL,
    tenant_id        UUID            NOT NULL,
    shipment_id      UUID            NOT NULL,
    task_id          UUID,
    driver_id        UUID,

    -- Event classification
    -- Known values: 'pickup_scan', 'delivery_attempt', 'pod_submitted', 'pop_submitted',
    --               'out_of_bounds_handover', 'otp_generated', 'otp_verified',
    --               'geofence_check', 'correction'
    event_type       TEXT            NOT NULL,

    -- Partition key = device_timestamp (canonical event time)
    event_time       TIMESTAMPTZ     NOT NULL,
    device_timestamp TIMESTAMPTZ     NOT NULL,
    server_timestamp TIMESTAMPTZ     NOT NULL DEFAULT NOW(),

    -- GPS coordinates at moment of event
    lat              DOUBLE PRECISION,
    lng              DOUBLE PRECISION,

    -- Freeform JSON payload — event-specific metadata
    -- Examples:
    --   pickup_scan:          { "scanned_barcode": "CM-PH1-...", "actual_weight_g": 2300 }
    --   out_of_bounds_handover: { "telemetry_exception": "OUT_OF_BOUNDS_HANDOVER",
    --                             "distance_m": 73.4 }
    --   correction:           { "original_event_id": 12345, "reason": "clock drift fix",
    --                           "drift_seconds": -320 }
    metadata         JSONB           NOT NULL DEFAULT '{}',

    PRIMARY KEY (id, event_time)
);

-- Convert to TimescaleDB hypertable (idempotent — safe to re-run).
-- Wrapping in a DO block so the migration succeeds on plain PostgreSQL too;
-- the hypertable creation error is swallowed and a warning logged instead.
DO $$
BEGIN
    PERFORM create_hypertable(
        'pod.shipment_telemetry_logs',
        'event_time',
        chunk_time_interval => INTERVAL '1 month',
        if_not_exists       => TRUE
    );
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING 'TimescaleDB not available — shipment_telemetry_logs will remain a plain table: %', SQLERRM;
END;
$$;

CREATE INDEX IF NOT EXISTS idx_telemetry_shipment
    ON pod.shipment_telemetry_logs (shipment_id, event_time DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_driver
    ON pod.shipment_telemetry_logs (driver_id, event_time DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_type
    ON pod.shipment_telemetry_logs (tenant_id, event_type, event_time DESC);
