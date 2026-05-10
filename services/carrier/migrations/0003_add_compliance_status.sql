-- Add compliance_status to carriers.
-- Tracks KYB / document verification state independently of operational status.
-- Valid values mirror the ComplianceStatus enum in domain/entities/mod.rs:
--   pending_submission | under_review | compliant | expiring_soon | expired | rejected | suspended
ALTER TABLE carrier.carriers
    ADD COLUMN IF NOT EXISTS compliance_status TEXT NOT NULL DEFAULT 'pending_submission';

COMMENT ON COLUMN carrier.carriers.compliance_status IS
    'KYB / document verification state. Set by admin compliance review; not updated by operational events.';
