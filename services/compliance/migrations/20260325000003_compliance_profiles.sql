CREATE TABLE compliance.compliance_profiles (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('driver','partner','merchant')),
    entity_id        UUID        NOT NULL,
    overall_status   TEXT        NOT NULL DEFAULT 'pending_submission',
    jurisdiction     TEXT        NOT NULL,
    last_reviewed_at TIMESTAMPTZ,
    reviewed_by      UUID,
    suspended_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, entity_type, entity_id)
);

CREATE INDEX idx_compliance_profiles_status
    ON compliance.compliance_profiles (tenant_id, overall_status);
CREATE INDEX idx_compliance_profiles_entity
    ON compliance.compliance_profiles (entity_type, entity_id);

-- Row-Level Security: isolate tenant data at DB layer (project standard)
ALTER TABLE compliance.compliance_profiles ENABLE ROW LEVEL SECURITY;

CREATE POLICY compliance_profiles_tenant_isolation
    ON compliance.compliance_profiles
    USING (tenant_id = current_setting('app.tenant_id', true)::uuid);

-- Allow service role (used by sqlx connection) to bypass RLS
CREATE POLICY compliance_profiles_service_role
    ON compliance.compliance_profiles
    TO logisticos_service
    USING (true)
    WITH CHECK (true);
