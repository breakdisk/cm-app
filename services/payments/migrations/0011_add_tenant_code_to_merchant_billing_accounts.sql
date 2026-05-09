-- Add tenant_code to merchant_billing_accounts so the monthly billing
-- cron can generate correctly prefixed invoice numbers without a
-- cross-service call to identity.
ALTER TABLE payments.merchant_billing_accounts
    ADD COLUMN IF NOT EXISTS tenant_code VARCHAR(10) NOT NULL DEFAULT '';
