-- Migration: 0007 — Identity: Push tokens for mobile apps

CREATE TABLE identity.push_tokens (
    id               UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id        UUID        NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    user_id          UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    token            TEXT        NOT NULL,
    platform         TEXT        NOT NULL,
    app              TEXT        NOT NULL,
    device_id        TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (tenant_id, token)
);

CREATE INDEX idx_push_tokens_user   ON identity.push_tokens (tenant_id, user_id);
CREATE INDEX idx_push_tokens_app    ON identity.push_tokens (tenant_id, app);

CREATE TRIGGER push_tokens_updated_at
    BEFORE UPDATE ON identity.push_tokens
    FOR EACH ROW EXECUTE FUNCTION identity.set_updated_at();

-- Row-Level Security (per ADR-0003)
ALTER TABLE identity.push_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.push_tokens FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON identity.push_tokens
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
