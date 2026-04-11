-- Migration: 0001 — Identity: Tenants table
-- Managed by sqlx-migrate

CREATE TABLE IF NOT EXISTS identity.tenants (
    id                UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    name              TEXT        NOT NULL,
    slug              TEXT        NOT NULL UNIQUE,
    subscription_tier TEXT        NOT NULL DEFAULT 'starter'
                                  CHECK (subscription_tier IN ('starter','growth','business','enterprise')),
    is_active         BOOLEAN     NOT NULL DEFAULT TRUE,
    owner_email       TEXT        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenants_slug     ON identity.tenants (slug);
CREATE INDEX IF NOT EXISTS idx_tenants_is_active ON identity.tenants (is_active) WHERE is_active = TRUE;

-- Auto-update updated_at on row change
CREATE OR REPLACE FUNCTION identity.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS tenants_updated_at ON identity.tenants;
CREATE TRIGGER tenants_updated_at
    BEFORE UPDATE ON identity.tenants
    FOR EACH ROW EXECUTE FUNCTION identity.set_updated_at();
