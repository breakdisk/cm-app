-- Migration: 0008 — Identity: External auth provider identities
--
-- Links internal identity.users rows to external identity providers
-- (Firebase, SAML, Google Workspace). Enables sign-in via Firebase while
-- keeping the LogisticOS JWT as the backend auth currency.
--
-- See docs/superpowers/specs/2026-04-15-firebase-to-logisticos-jwt-bridge-design.md

CREATE TABLE identity.auth_identities (
    id                UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id           UUID        NOT NULL REFERENCES identity.users(id) ON DELETE CASCADE,
    provider          TEXT        NOT NULL
                                  CHECK (provider IN ('firebase','saml','google_workspace')),
    provider_subject  TEXT        NOT NULL,
    email_at_link     TEXT        NOT NULL,
    linked_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (provider, provider_subject)
);

CREATE INDEX idx_auth_identities_user     ON identity.auth_identities (user_id);
CREATE INDEX idx_auth_identities_provider ON identity.auth_identities (provider, provider_subject);

-- ── Row-Level Security (per ADR-0003) ────────────────────────
ALTER TABLE identity.auth_identities ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity.auth_identities FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON identity.auth_identities
    USING (user_id IN (
        SELECT id FROM identity.users
        WHERE tenant_id = current_setting('app.tenant_id')::uuid
    ));

-- Firebase-originated users have no LogisticOS password — make it optional.
-- Existing rows unaffected.
ALTER TABLE identity.users
    ALTER COLUMN password_hash DROP NOT NULL;
