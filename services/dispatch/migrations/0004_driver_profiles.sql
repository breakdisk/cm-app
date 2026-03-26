-- driver_profiles: local cache of driver identities in the dispatch service.
-- Populated by consuming USER_CREATED events from identity service
-- where role contains 'driver'.
CREATE TABLE IF NOT EXISTS dispatch.driver_profiles (
    id          UUID        PRIMARY KEY,  -- Same UUID as identity.users.id
    tenant_id   UUID        NOT NULL,
    email       TEXT        NOT NULL,
    first_name  TEXT        NOT NULL DEFAULT '',
    last_name   TEXT        NOT NULL DEFAULT '',
    is_active   BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_driver_profiles_tenant
    ON dispatch.driver_profiles (tenant_id, is_active);
