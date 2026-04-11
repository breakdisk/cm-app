-- Migration: 0003 — Identity: API keys table

CREATE TABLE IF NOT EXISTS identity.api_keys (
    id            UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id     UUID        NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    name          TEXT        NOT NULL,
    key_hash      TEXT        NOT NULL UNIQUE,   -- SHA-256 of the raw key
    key_prefix    TEXT        NOT NULL,           -- First 8 chars for display
    scopes        TEXT[]      NOT NULL DEFAULT '{}',
    is_active     BOOLEAN     NOT NULL DEFAULT TRUE,
    expires_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id  ON identity.api_keys (tenant_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash   ON identity.api_keys (key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_is_active  ON identity.api_keys (tenant_id, is_active) WHERE is_active = TRUE;

ALTER TABLE identity.api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.api_keys FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_isolation ON identity.api_keys;
CREATE POLICY tenant_isolation ON identity.api_keys
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
