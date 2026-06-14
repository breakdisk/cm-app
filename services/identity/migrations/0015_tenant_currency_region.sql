-- Migration: 0015 — Identity: Tenant currency and region
--
-- Completes the FinalizeTenantCommand wire. currency (ISO 4217) and region
-- (ISO 3166-1 alpha-2) were validated on intake from day one but only logged,
-- never persisted. See the comment in tenant_service.rs::finalize_self that
-- explicitly marked this as the pending follow-up migration.
--
-- NULL for existing active tenants (created via the direct-API path before
-- this migration). Only finalize-path tenants (Firebase lazy onboarding) will
-- have non-NULL values going forward.

ALTER TABLE identity.tenants
    ADD COLUMN IF NOT EXISTS currency TEXT,
    ADD COLUMN IF NOT EXISTS region   TEXT;
