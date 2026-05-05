CREATE TABLE IF NOT EXISTS identity.pickup_addresses (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    label       TEXT        NOT NULL,
    address     TEXT        NOT NULL,
    city        TEXT        NOT NULL DEFAULT '',
    province    TEXT        NOT NULL DEFAULT '',
    postal_code TEXT        NOT NULL DEFAULT '',
    country     TEXT        NOT NULL DEFAULT 'PH',
    lat         DOUBLE PRECISION,
    lng         DOUBLE PRECISION,
    is_default  BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pickup_addresses_user_id   ON identity.pickup_addresses (user_id);
CREATE INDEX IF NOT EXISTS idx_pickup_addresses_tenant_id ON identity.pickup_addresses (tenant_id);

ALTER TABLE identity.pickup_addresses ENABLE ROW LEVEL SECURITY;

CREATE POLICY pickup_addresses_tenant_isolation ON identity.pickup_addresses
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
