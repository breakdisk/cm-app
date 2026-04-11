CREATE TABLE compliance.driver_documents (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    compliance_profile_id UUID        NOT NULL REFERENCES compliance.compliance_profiles(id),
    document_type_id      UUID        NOT NULL REFERENCES compliance.document_types(id),
    document_number       TEXT        NOT NULL,
    issue_date            DATE,
    expiry_date           DATE,
    file_url              TEXT        NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'submitted',
    rejection_reason      TEXT,
    reviewed_by           UUID,
    reviewed_at           TIMESTAMPTZ,
    submitted_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_driver_documents_profile
    ON compliance.driver_documents (compliance_profile_id);
CREATE INDEX idx_driver_documents_expiry
    ON compliance.driver_documents (expiry_date)
    WHERE status = 'approved';
