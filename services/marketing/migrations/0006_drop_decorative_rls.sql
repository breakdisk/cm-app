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

DROP POLICY IF EXISTS marketing_tenant_isolation ON marketing.campaigns;
DROP POLICY IF EXISTS tenant_rls ON marketing.send_log;
DROP POLICY IF EXISTS tenant_rls ON marketing.ab_tests;
DROP POLICY IF EXISTS tenant_rls ON marketing.journeys;
DROP POLICY IF EXISTS tenant_rls ON marketing.journey_enrollments;

ALTER TABLE IF EXISTS marketing.campaigns DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS marketing.send_log DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS marketing.ab_tests DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS marketing.journeys DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS marketing.journey_enrollments DISABLE ROW LEVEL SECURITY;
