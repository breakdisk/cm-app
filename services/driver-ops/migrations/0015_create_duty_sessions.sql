-- Hours of service (mobile design, compliance screen): one row per stretch on
-- duty, opened by go-online and closed by go-offline or by an admin forcing the
-- driver offline. Display and record only — nothing reads this to refuse work.
CREATE TABLE IF NOT EXISTS driver_ops.duty_sessions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    driver_id   UUID        NOT NULL REFERENCES driver_ops.drivers (id) ON DELETE CASCADE,
    started_at  TIMESTAMPTZ NOT NULL,
    ended_at    TIMESTAMPTZ,
    CONSTRAINT duty_sessions_end_after_start CHECK (ended_at IS NULL OR ended_at >= started_at)
);

-- At most one open session per driver. go-online is repeated (the toggle, restore
-- on reconnect) and must not stack sessions that double-count the clock; the
-- repository's INSERT … ON CONFLICT (driver_id) WHERE ended_at IS NULL relies on
-- exactly this index.
CREATE UNIQUE INDEX IF NOT EXISTS duty_sessions_one_open_per_driver
    ON driver_ops.duty_sessions (driver_id) WHERE ended_at IS NULL;

-- The clock's read: one driver's sessions overlapping the last N hours.
CREATE INDEX IF NOT EXISTS duty_sessions_tenant_driver_started
    ON driver_ops.duty_sessions (tenant_id, driver_id, started_at DESC);
