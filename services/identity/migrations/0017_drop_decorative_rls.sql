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

DROP POLICY IF EXISTS tenant_isolation ON identity.users;
DROP POLICY IF EXISTS tenant_isolation ON identity.api_keys;
DROP POLICY IF EXISTS tenant_isolation ON identity.push_tokens;
DROP POLICY IF EXISTS tenant_isolation ON identity.auth_identities;
DROP POLICY IF EXISTS pickup_addresses_tenant_isolation ON identity.pickup_addresses;
DROP POLICY IF EXISTS tenant_branding_tenant_isolation ON identity.tenant_branding;

ALTER TABLE IF EXISTS identity.users DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS identity.api_keys DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS identity.push_tokens DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS identity.auth_identities DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS identity.pickup_addresses DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS identity.tenant_branding DISABLE ROW LEVEL SECURITY;
