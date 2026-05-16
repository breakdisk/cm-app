-- Fix RLS key mismatch on marketing.send_log and marketing.ab_tests.
-- Both tables were using 'app.current_tenant_id' while every other marketing
-- table and the bootstrap after_connect hook set 'app.tenant_id'.
ALTER TABLE marketing.send_log  DROP POLICY IF EXISTS tenant_rls;
ALTER TABLE marketing.ab_tests  DROP POLICY IF EXISTS tenant_rls;

CREATE POLICY tenant_rls ON marketing.send_log
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid);

CREATE POLICY tenant_rls ON marketing.ab_tests
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid);
