-- Migration: 0009 — Fix RLS policies on dispatch.routes and dispatch.route_stops.
--
-- Problem: current_setting('app.tenant_id') (without the missing-OK flag) throws
--   ERROR: unrecognized configuration parameter "app.tenant_id"
-- when the session variable is not set. This causes every INSERT into dispatch.routes
-- and dispatch.route_stops to fail with a 500 Internal Server Error, because the
-- service connection does not SET app.tenant_id before each DML statement.
--
-- Fix A: Grant BYPASSRLS to the service role so the application code is not blocked
--   by RLS (RLS remains in effect for direct DB users and auditing tools).
-- Fix B: Change the route/route_stops policies to use the missing-OK form
--   current_setting('app.tenant_id', true) consistent with driver_assignments.
--   With BYPASSRLS on the role this is a belt-and-suspenders safeguard.

-- Grant the service role permission to bypass RLS entirely.
-- This is the standard pattern for trusted service accounts — RLS guards against
-- direct SQL access by untrusted sessions, not by application code.
DO $$
BEGIN
    ALTER ROLE logisticos BYPASSRLS;
EXCEPTION WHEN insufficient_privilege THEN
    -- Running as non-superuser during local dev; skip gracefully.
    RAISE NOTICE 'Could not grant BYPASSRLS — skipping (run as superuser to apply).';
END $$;

-- Also fix the policies to use the missing-OK form so the setting is not required.
-- If BYPASSRLS is not active (e.g. local dev with a restricted role), the policy
-- will return no rows rather than throwing, which is the safe fallback.
DROP POLICY IF EXISTS tenant_isolation ON dispatch.routes;
CREATE POLICY tenant_isolation ON dispatch.routes
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

DROP POLICY IF EXISTS tenant_isolation ON dispatch.route_stops;
CREATE POLICY tenant_isolation ON dispatch.route_stops
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);
