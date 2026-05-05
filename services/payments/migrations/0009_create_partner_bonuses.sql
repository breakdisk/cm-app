CREATE TABLE payments.partner_bonuses (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id        UUID NOT NULL,
  partner_id       UUID NOT NULL,
  amount_centavos  BIGINT NOT NULL,
  currency         CHAR(3) NOT NULL DEFAULT 'PHP',
  reason           TEXT NOT NULL,
  effective_month  DATE NOT NULL,
  created_by       UUID NOT NULL,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_partner_bonuses_tenant_partner_month
    ON payments.partner_bonuses (tenant_id, partner_id, effective_month);

ALTER TABLE payments.partner_bonuses ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON payments.partner_bonuses
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);
