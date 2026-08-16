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

DROP POLICY IF EXISTS analytics_tenant_isolation ON analytics.shipment_events;
DROP POLICY IF EXISTS analytics_daily_kpis_tenant_isolation ON analytics.daily_kpis;
DROP POLICY IF EXISTS analytics_driver_daily_tenant_isolation ON analytics.driver_daily_stats;
DROP POLICY IF EXISTS analytics_zone_daily_tenant_isolation ON analytics.zone_daily_stats;

ALTER TABLE IF EXISTS analytics.shipment_events DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS analytics.daily_kpis DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS analytics.driver_daily_stats DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS analytics.zone_daily_stats DISABLE ROW LEVEL SECURITY;
