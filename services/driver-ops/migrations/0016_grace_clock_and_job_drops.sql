-- Mover drop penalties and the waiting grace clock (mobile design part 2, C2).

-- 1. 'cancelled' becomes a task state.
--
-- `cancel_all_for_driver` (admin POST /v1/drivers/:id/cancel-tasks) has always
-- written status = 'cancelled', which the CHECK in 0002 refuses, so that
-- endpoint failed on any driver with an open task. A dropped job needs the same
-- state. The constraint is found by definition rather than by name, because an
-- inline CHECK's name is generated.
DO $$
DECLARE c record;
BEGIN
    FOR c IN
        SELECT conname FROM pg_constraint
        WHERE conrelid = 'driver_ops.tasks'::regclass
          AND contype = 'c'
          AND pg_get_constraintdef(oid) LIKE '%(status = ANY%'
    LOOP
        EXECUTE format('ALTER TABLE driver_ops.tasks DROP CONSTRAINT %I', c.conname);
    END LOOP;
END $$;

ALTER TABLE driver_ops.tasks
    ADD CONSTRAINT tasks_status_check
    CHECK (status IN ('pending', 'in_progress', 'completed', 'failed', 'skipped', 'cancelled'));

-- 2. The grace clock. Stamped by the server when the driver arrives (task
-- start), from PENALTY__GRACE_MINUTES at that moment, so a config change never
-- moves a clock that is already running. NULL = no clock (switched off, or the
-- driver has not arrived).
ALTER TABLE driver_ops.tasks
    ADD COLUMN IF NOT EXISTS grace_expires_at  TIMESTAMPTZ,
    -- Paid to the driver when they release a stop after grace. Snapshotted at
    -- release, like payout_cents at claim.
    ADD COLUMN IF NOT EXISTS waiting_fee_cents BIGINT NOT NULL DEFAULT 0;

-- 3. One row per shipment a driver dropped. The fee is what earnings deduct;
-- the row is also the acceptance-rate input and the audit record.
CREATE TABLE IF NOT EXISTS driver_ops.job_drops (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    -- drivers.id, which 0014 makes equal to the identity user id.
    driver_id    UUID        NOT NULL,
    route_id     UUID        NOT NULL,
    shipment_id  UUID        NOT NULL,
    -- Named on the driver's earnings line.
    tracking_number TEXT,
    reason_code  TEXT        NOT NULL,
    note         TEXT,
    lat          DOUBLE PRECISION,
    lng          DOUBLE PRECISION,
    -- The payout the fee was taken from, and the fee, both in cents.
    payout_cents BIGINT      NOT NULL DEFAULT 0,
    fee_cents    BIGINT      NOT NULL DEFAULT 0,
    dropped_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- A job is dropped once per assignment. A retried request finds this row
    -- and replays the event instead of charging twice; being assigned the same
    -- shipment again later is a new route, so a second drop is still recorded.
    CONSTRAINT job_drops_once UNIQUE (driver_id, shipment_id, route_id),
    CONSTRAINT job_drops_fee_not_negative CHECK (fee_cents >= 0)
);

-- Earnings read a driver's drops by time.
CREATE INDEX IF NOT EXISTS job_drops_by_driver_time
    ON driver_ops.job_drops (driver_id, dropped_at DESC);
