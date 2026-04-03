-- Migration: 0006 — email_verification_tokens table

CREATE TABLE IF NOT EXISTS identity.email_verification_tokens (
    id          UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    token_hash  TEXT        NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    used        BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_evt_token_hash ON identity.email_verification_tokens (token_hash);
CREATE INDEX IF NOT EXISTS idx_evt_user_id    ON identity.email_verification_tokens (user_id);
