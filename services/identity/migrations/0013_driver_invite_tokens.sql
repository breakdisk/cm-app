-- Migration: 0013_driver_invite_tokens
-- Stores signed driver invite link tokens.
-- Each row represents one generated link; used_at is set on first tap.
-- The HMAC in the URL is stateless — this table is belt-and-braces audit + single-use guard.

CREATE TABLE IF NOT EXISTS identity.driver_invite_tokens (
    id            UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id     UUID        NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    user_id       UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    -- SHA-256 hex digest of the raw HMAC sig included in the invite URL.
    -- Unique so a second generate for the same driver produces a new row.
    token_hash    TEXT        NOT NULL UNIQUE,
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_driver_invite_tokens_user_id    ON identity.driver_invite_tokens (user_id);
CREATE INDEX IF NOT EXISTS idx_driver_invite_tokens_expires_at ON identity.driver_invite_tokens (expires_at);

-- Prevent application role from altering or deleting token rows (audit integrity).
-- Uncomment once the app_role is provisioned:
-- REVOKE UPDATE, DELETE ON identity.driver_invite_tokens FROM app_role;
