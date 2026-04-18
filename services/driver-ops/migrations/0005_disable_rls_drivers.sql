-- Disable RLS on driver_ops.drivers.
-- The policy was created in 0001 but app.tenant_id is never set in the service's
-- DB session, causing current_setting('app.tenant_id', true) to return NULL and
-- silently filtering every row from SELECT queries (go_online, find_by_user_id, etc).
-- Tenant isolation is enforced at the application layer via JWT claims and explicit
-- WHERE tenant_id = $n clauses in every repository query.
ALTER TABLE driver_ops.drivers DISABLE ROW LEVEL SECURITY;
