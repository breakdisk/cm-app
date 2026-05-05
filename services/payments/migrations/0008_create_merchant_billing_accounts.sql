CREATE TABLE payments.merchant_billing_accounts (
  id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id                   UUID NOT NULL,
  merchant_id                 UUID NOT NULL UNIQUE,
  base_rate_override_centavos BIGINT,
  payment_terms_days          SMALLINT NOT NULL DEFAULT 30,
  credit_limit_centavos       BIGINT NOT NULL DEFAULT 0,
  tin                         VARCHAR(20),
  vat_registered              BOOLEAN NOT NULL DEFAULT false,
  billing_email               TEXT NOT NULL,
  invoice_channel             TEXT NOT NULL DEFAULT 'email',
  bank_name                   TEXT,
  bank_account_number         TEXT,
  bank_account_name           TEXT,
  created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_merchant_billing_accounts_tenant
    ON payments.merchant_billing_accounts (tenant_id);

ALTER TABLE payments.merchant_billing_accounts ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.merchant_billing_accounts
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);
