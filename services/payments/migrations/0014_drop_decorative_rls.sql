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

DROP POLICY IF EXISTS tenant_isolation ON payments.invoices;
DROP POLICY IF EXISTS tenant_isolation ON payments.cod_collections;
DROP POLICY IF EXISTS tenant_isolation ON payments.wallets;
DROP POLICY IF EXISTS tenant_isolation ON payments.wallet_transactions;
DROP POLICY IF EXISTS cod_records_tenant_isolation ON payments.cod_records;
DROP POLICY IF EXISTS cod_batches_tenant_isolation ON payments.cod_batches;
DROP POLICY IF EXISTS wallets_tenant_isolation ON payments.wallets;
DROP POLICY IF EXISTS wallet_txn_tenant_isolation ON payments.wallet_transactions;
DROP POLICY IF EXISTS tenant_isolation ON payments.invoice_line_items;
DROP POLICY IF EXISTS tenant_isolation ON payments.invoice_adjustments;
DROP POLICY IF EXISTS tenant_isolation ON payments.billing_runs;
DROP POLICY IF EXISTS tenant_isolation ON payments.cod_remittance_batches;
DROP POLICY IF EXISTS tenant_isolation ON payments.merchant_billing_accounts;
DROP POLICY IF EXISTS tenant_isolation ON payments.partner_bonuses;
DROP POLICY IF EXISTS tenant_isolation ON payments.withdrawal_requests;
DROP POLICY IF EXISTS driver_ledgers_service_policy ON payments.driver_ledgers;
DROP POLICY IF EXISTS driver_ledger_entries_service_policy ON payments.driver_ledger_entries;

ALTER TABLE IF EXISTS payments.invoices DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.cod_collections DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.wallets DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.wallet_transactions DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.cod_records DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.cod_batches DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.invoice_line_items DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.invoice_adjustments DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.billing_runs DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.cod_remittance_batches DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.merchant_billing_accounts DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.partner_bonuses DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.withdrawal_requests DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.driver_ledgers DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS payments.driver_ledger_entries DISABLE ROW LEVEL SECURITY;
