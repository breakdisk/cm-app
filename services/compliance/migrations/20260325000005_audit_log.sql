CREATE TABLE compliance.compliance_audit_log (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             UUID        NOT NULL,
    compliance_profile_id UUID        NOT NULL REFERENCES compliance.compliance_profiles(id),
    document_id           UUID        REFERENCES compliance.driver_documents(id) ON DELETE SET NULL,
    event_type            TEXT        NOT NULL,
    actor_id              UUID        NOT NULL,
    actor_type            TEXT        NOT NULL CHECK (actor_type IN ('driver','admin','system')),
    notes                 TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_profile
    ON compliance.compliance_audit_log (compliance_profile_id, created_at DESC);
CREATE INDEX idx_audit_log_tenant
    ON compliance.compliance_audit_log (tenant_id, created_at DESC);
