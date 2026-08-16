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

DROP POLICY IF EXISTS rls_automation_rules_tenant_isolation ON business_logic.automation_rules;
DROP POLICY IF EXISTS rls_automation_rules_service_bypass ON business_logic.automation_rules;
DROP POLICY IF EXISTS rls_rule_executions_tenant_isolation ON business_logic.rule_executions;
DROP POLICY IF EXISTS rls_rule_executions_service_bypass ON business_logic.rule_executions;
DROP POLICY IF EXISTS rls_workflow_instances_tenant_isolation ON business_logic.workflow_instances;
DROP POLICY IF EXISTS rls_workflow_instances_service_bypass ON business_logic.workflow_instances;
DROP POLICY IF EXISTS rls_workflow_step_logs_via_workflow ON business_logic.workflow_step_logs;
DROP POLICY IF EXISTS rls_workflow_step_logs_service_bypass ON business_logic.workflow_step_logs;

ALTER TABLE IF EXISTS business_logic.automation_rules DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS business_logic.rule_executions DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS business_logic.workflow_instances DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS business_logic.workflow_step_logs DISABLE ROW LEVEL SECURITY;
