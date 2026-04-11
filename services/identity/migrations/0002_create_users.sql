-- Migration: 0002 — Identity: Users table with RLS

CREATE TABLE IF NOT EXISTS identity.users (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id        UUID        NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    email            TEXT        NOT NULL,
    password_hash    TEXT        NOT NULL,
    first_name       TEXT        NOT NULL,
    last_name        TEXT        NOT NULL,
    roles            TEXT[]      NOT NULL DEFAULT '{}',
    is_active        BOOLEAN     NOT NULL DEFAULT TRUE,
    email_verified   BOOLEAN     NOT NULL DEFAULT FALSE,
    last_login_at    TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (tenant_id, email)
);

CREATE INDEX IF NOT EXISTS idx_users_tenant_id  ON identity.users (tenant_id);
CREATE INDEX IF NOT EXISTS idx_users_email      ON identity.users (tenant_id, email);
CREATE INDEX IF NOT EXISTS idx_users_is_active  ON identity.users (tenant_id, is_active) WHERE is_active = TRUE;

DROP TRIGGER IF EXISTS users_updated_at ON identity.users;
CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON identity.users
    FOR EACH ROW EXECUTE FUNCTION identity.set_updated_at();

-- ── Row-Level Security (per ADR-0003) ────────────────────────
ALTER TABLE identity.users ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.users FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_isolation ON identity.users;
CREATE POLICY tenant_isolation ON identity.users
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
