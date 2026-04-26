CREATE TABLE IF NOT EXISTS identity.tenant_audit_log (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    actor_id    UUID,
    actor_email TEXT,
    action      TEXT        NOT NULL,
    resource    TEXT        NOT NULL,
    ip          TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS tenant_audit_log_tenant_time_idx
    ON identity.tenant_audit_log (tenant_id, created_at DESC);
