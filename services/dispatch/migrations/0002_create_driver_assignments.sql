-- Driver assignments: binding a driver to a route.
-- A driver can only have one pending/accepted assignment at a time (enforced by partial unique index).

CREATE TABLE IF NOT EXISTS dispatch.driver_assignments (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    driver_id        UUID        NOT NULL,
    route_id         UUID        NOT NULL REFERENCES dispatch.routes(id) ON DELETE CASCADE,
    status           TEXT        NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled')),
    assigned_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at      TIMESTAMPTZ,
    rejected_at      TIMESTAMPTZ,
    rejection_reason TEXT
);

-- Only one active assignment per driver at a time
CREATE UNIQUE INDEX IF NOT EXISTS uq_driver_active_assignment
    ON dispatch.driver_assignments (driver_id)
    WHERE status IN ('pending', 'accepted');

CREATE INDEX IF NOT EXISTS idx_assignments_route_id
    ON dispatch.driver_assignments (route_id);

-- RLS: tenants can only see their own assignments
ALTER TABLE dispatch.driver_assignments ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_isolation ON dispatch.driver_assignments;
DROP POLICY IF EXISTS tenant_isolation ON dispatch.driver_assignments;
CREATE POLICY tenant_isolation ON dispatch.driver_assignments
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Trigger to update route status when assignment is accepted
CREATE OR REPLACE FUNCTION dispatch.on_assignment_accepted()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'accepted' AND OLD.status = 'pending' THEN
        UPDATE dispatch.routes
           SET status = 'in_progress', started_at = NOW()
         WHERE id = NEW.route_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_assignment_accepted ON dispatch.driver_assignments;
DROP TRIGGER IF EXISTS trg_assignment_accepted ON dispatch.driver_assignments;
CREATE TRIGGER trg_assignment_accepted
    AFTER UPDATE ON dispatch.driver_assignments
    FOR EACH ROW
    WHEN (OLD.status = 'pending' AND NEW.status = 'accepted')
    EXECUTE FUNCTION dispatch.on_assignment_accepted();
