-- Fix RLS key mismatch on marketing.send_log and marketing.ab_tests.
-- Both tables were using 'app.current_tenant_id' while every other marketing
-- table and the bootstrap after_connect hook set 'app.tenant_id'.
-- `ALTER TABLE ... DROP POLICY` is not PostgreSQL syntax — DROP POLICY is its
-- own statement. This file therefore failed on its first statement and has
-- never applied anywhere, which silently blocked 0004, 0005 and 0006 behind it:
-- on the production VPS `_sqlx_migrations` stops at version 2, so
-- marketing.journeys, journey_steps and journey_enrollments do not exist and
-- the journey features built against them cannot work.
--
-- Corrected to the same form 0006 already uses. Only the syntax changed; the
-- intent is untouched. Safe to edit in place precisely because it never applied
-- — there is no recorded checksum anywhere to conflict with.
DROP POLICY IF EXISTS tenant_rls ON marketing.send_log;
DROP POLICY IF EXISTS tenant_rls ON marketing.ab_tests;

CREATE POLICY tenant_rls ON marketing.send_log
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid);

CREATE POLICY tenant_rls ON marketing.ab_tests
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid);
