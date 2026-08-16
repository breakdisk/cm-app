-- Drop decorative Row-Level Security. See ADR-0016.
--
-- These policies never enforced anything: no service sets
-- current_setting('app.tenant_id'), and services connect as the schema
-- owner, which PostgreSQL exempts from RLS unless FORCE is set. Where FORCE
-- was tried it broke reads and was reverted (order-intake/0007,
-- driver-ops/0005).
--
-- Tenant isolation is application-layer: every repository method takes
-- tenant_id and every statement filters on it. Removing these stops the
-- schema asserting a database guarantee that does not exist.
--
-- Observably a no-op at runtime, which is why it is safe in one pass.

DROP POLICY IF EXISTS tracking_tenant_isolation ON tracking.shipment_tracking;
DROP POLICY IF EXISTS tenant_rls ON delivery_experience.tracking_events;
DROP POLICY IF EXISTS tenant_rls ON delivery_experience.delivery_preferences;
DROP POLICY IF EXISTS tenant_isolation ON delivery_experience.tracking;

ALTER TABLE IF EXISTS tracking.shipment_tracking DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS delivery_experience.tracking_events DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS delivery_experience.delivery_preferences DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS delivery_experience.tracking DISABLE ROW LEVEL SECURITY;
