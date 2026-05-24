-- Add customer-facing document type codes used by the customer-app KYC screen.
--
-- The original seed (20260325000006) only covered driver document types with
-- jurisdiction-prefixed codes (UAE_EMIRATES_ID, PH_LTO_LICENSE, etc.).
-- The customer-app KYCScreen hardcodes two simpler codes:
--   • "passport"    (Philippine passport — required for international shipments)
--   • "emirates_id" (UAE identity card  — accepted for local shipments only)
--
-- find_by_code() uses an exact-match WHERE clause, so these codes must exist
-- verbatim. Seeded with applicable_to = '{customer}' so they don't appear in
-- driver compliance checks. has_expiry is false for passports (expiry
-- validation is left to the admin review step); true for Emirates IDs.
--
-- ON CONFLICT DO NOTHING makes this migration safe to re-run.

INSERT INTO compliance.document_types
    (code, jurisdiction, applicable_to, name, description,
     is_required, has_expiry, warn_days_before, grace_period_days)
VALUES
  (
    'passport',
    'PH',
    '{customer}',
    'Philippine Passport',
    'Valid for local and international/Balikbayan shipments. Biodata page required.',
    true,
    false,
    0,
    0
  ),
  (
    'emirates_id',
    'UAE',
    '{customer}',
    'Emirates ID',
    'UAE identity card. Accepted for local (domestic) shipments only.',
    false,
    true,
    60,
    14
  )
ON CONFLICT (code) DO NOTHING;
