-- Migration 0011: add phone_number to identity.users
-- Enables driver OTP login to resolve pre-registered users by phone
-- rather than falling back to synthetic-email auto-registration.
--
-- Stored in E.164-normalised form (e.g. +639171234567) so lookups are
-- deterministic regardless of how the admin typed the number at registration.

ALTER TABLE identity.users
    ADD COLUMN IF NOT EXISTS phone_number TEXT;

-- Partial unique index: a phone number may only be registered once per tenant.
-- NULL values are excluded so unset phones don't collide.
CREATE UNIQUE INDEX IF NOT EXISTS uq_users_tenant_phone
    ON identity.users (tenant_id, phone_number)
    WHERE phone_number IS NOT NULL;
