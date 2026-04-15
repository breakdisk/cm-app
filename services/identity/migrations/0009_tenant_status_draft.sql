-- Migration: 0009 — Identity: Tenant lifecycle status
--
-- Adds a `status` column to support the lazy-onboarding flow. A merchant
-- signing in via Firebase for the first time gets a `draft` tenant with
-- onboarding-only permissions, then finalizes via POST /v1/tenants/me/finalize.
--
-- See docs/superpowers/specs/2026-04-15-firebase-to-logisticos-jwt-bridge-design.md

DO $$ BEGIN
    CREATE TYPE identity.tenant_status AS ENUM ('draft', 'active', 'suspended');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

ALTER TABLE identity.tenants
    ADD COLUMN IF NOT EXISTS status identity.tenant_status NOT NULL DEFAULT 'active';

-- Existing tenants stay `active`. Only newly provisioned draft tenants
-- start as `draft` (set explicitly by the provisioning path in auth_service).

CREATE INDEX IF NOT EXISTS idx_tenants_status
    ON identity.tenants (status)
    WHERE status <> 'active';
