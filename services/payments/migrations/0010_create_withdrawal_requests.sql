CREATE TYPE payments.withdrawal_status AS ENUM ('pending', 'approved', 'disbursed', 'rejected');

CREATE TABLE payments.withdrawal_requests (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       UUID NOT NULL,
  wallet_id       UUID NOT NULL REFERENCES payments.wallets(id),
  amount_centavos BIGINT NOT NULL,
  currency        CHAR(3) NOT NULL DEFAULT 'PHP',
  status          payments.withdrawal_status NOT NULL DEFAULT 'pending',
  requested_by    UUID NOT NULL,
  reviewed_by     UUID,
  review_note     TEXT,
  reviewed_at     TIMESTAMPTZ,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_withdrawal_requests_tenant_status
    ON payments.withdrawal_requests (tenant_id, status);

CREATE INDEX idx_withdrawal_requests_wallet
    ON payments.withdrawal_requests (wallet_id);

ALTER TABLE payments.withdrawal_requests ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.withdrawal_requests
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

ALTER TABLE payments.wallets
  ADD COLUMN reserved_centavos BIGINT NOT NULL DEFAULT 0;
