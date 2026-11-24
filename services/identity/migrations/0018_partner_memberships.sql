-- Which partner (carrier or neighborhood store) a user acts for. Source of the
-- `pid` JWT claim; every partner-scoped read in other services filters on it.
-- Tenant isolation is application-layer (ADR-0016), so there is no RLS here.
CREATE TABLE IF NOT EXISTS identity.partner_memberships (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    user_id     UUID        NOT NULL,
    carrier_id  UUID        NOT NULL,
    role        TEXT        NOT NULL,
    status      TEXT        NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, user_id, carrier_id)
);

-- A user acts for one partner at a time. Without this, token issue would have
-- to pick one of several, and a wrong pick would scope a user to someone
-- else's data. Enforced by the database so two racing approvals cannot both win.
CREATE UNIQUE INDEX IF NOT EXISTS uq_one_active_partner_membership
    ON identity.partner_memberships (tenant_id, user_id)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_partner_memberships_carrier
    ON identity.partner_memberships (tenant_id, carrier_id);
