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

DROP POLICY IF EXISTS cdp_profiles_tenant_isolation ON cdp.customer_profiles;
DROP POLICY IF EXISTS tenant_rls ON cdp.segments;
DROP POLICY IF EXISTS tenant_rls ON cdp.consent_records;
DROP POLICY IF EXISTS tenant_rls ON cdp.churn_scores;

ALTER TABLE IF EXISTS cdp.customer_profiles DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS cdp.segments DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS cdp.consent_records DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS cdp.churn_scores DISABLE ROW LEVEL SECURITY;
