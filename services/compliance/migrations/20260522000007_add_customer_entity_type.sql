-- Add 'customer' to the allowed entity_type values on compliance_profiles.
-- The customer-app KYC upload flow calls ensure_profile(..., "customer", ...)
-- which was violating the original ('driver','partner','merchant') constraint.
ALTER TABLE compliance.compliance_profiles
    DROP CONSTRAINT compliance_profiles_entity_type_check;

ALTER TABLE compliance.compliance_profiles
    ADD CONSTRAINT compliance_profiles_entity_type_check
    CHECK (entity_type IN ('driver', 'partner', 'merchant', 'customer'));
