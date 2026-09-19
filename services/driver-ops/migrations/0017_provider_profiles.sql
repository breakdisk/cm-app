-- Migration: 0017 — which jobs a driver is onboarded for, and when they work
--
-- Decided 2026-09-19: freight moves and whole-home moves are separate service
-- lines; a home move goes only to a lead onboarded for it; coverage is local
-- or international. A multi-truck (Enterprise) lead brings more than one
-- truck. Operations set the capabilities at onboarding; the lead sets their
-- own working days and off days.
--
-- A driver with no row is a freight driver on the defaults, so nobody
-- onboarded before this changes: they keep every freight job, and no home
-- move is offered to them.

CREATE TABLE IF NOT EXISTS driver_ops.provider_profiles (
    driver_id           UUID        PRIMARY KEY REFERENCES driver_ops.drivers(id) ON DELETE CASCADE,
    tenant_id           UUID        NOT NULL,
    service_lines       TEXT[]      NOT NULL DEFAULT '{freight_move}'
                        CHECK (cardinality(service_lines) > 0 AND service_lines <@ ARRAY['freight_move','home_move']),
    coverage            TEXT[]      NOT NULL DEFAULT '{local}'
                        CHECK (cardinality(coverage) > 0 AND coverage <@ ARRAY['local','international']),
    multi_truck_capable BOOLEAN     NOT NULL DEFAULT FALSE,
    fleet_trucks        INTEGER     NOT NULL DEFAULT 1 CHECK (fleet_trucks BETWEEN 1 AND 20),
    registered_helpers  INTEGER     NOT NULL DEFAULT 0 CHECK (registered_helpers BETWEEN 0 AND 60),
    max_daily_jobs      INTEGER     NOT NULL DEFAULT 1 CHECK (max_daily_jobs BETWEEN 1 AND 4),
    -- 0 = Monday … 6 = Sunday.
    working_days        SMALLINT[]  NOT NULL DEFAULT '{0,1,2,3,4,5}',
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (NOT multi_truck_capable OR fleet_trucks >= 2)
);

CREATE INDEX IF NOT EXISTS idx_provider_profiles_home
    ON driver_ops.provider_profiles (tenant_id) WHERE 'home_move' = ANY(service_lines);

CREATE TABLE IF NOT EXISTS driver_ops.provider_off_days (
    driver_id UUID NOT NULL REFERENCES driver_ops.drivers(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL,
    day       DATE NOT NULL,
    PRIMARY KEY (driver_id, day)
);
