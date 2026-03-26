INSERT INTO compliance.document_types
    (code, jurisdiction, applicable_to, name, is_required, has_expiry, warn_days_before, grace_period_days)
VALUES
  ('UAE_EMIRATES_ID',       'UAE', '{driver}', 'Emirates ID',                  true, true, 60, 14),
  ('UAE_DRIVING_LICENSE',   'UAE', '{driver}', 'UAE Driving License',          true, true, 30,  7),
  ('UAE_VEHICLE_MULKIYA',   'UAE', '{driver}', 'Vehicle Registration (Mulkiya)',true, true, 30,  7),
  ('UAE_VEHICLE_INSURANCE', 'UAE', '{driver}', 'Third-Party Insurance',        true, true, 30,  7),
  ('PH_LTO_LICENSE',        'PH',  '{driver}', 'LTO Driving License',          true, true, 30,  7),
  ('PH_OR_CR',              'PH',  '{driver}', 'Vehicle OR/CR',                true, true, 30,  7),
  ('PH_NBI_CLEARANCE',      'PH',  '{driver}', 'NBI Clearance',                true, true, 60, 14),
  ('PH_CTPL_INSURANCE',     'PH',  '{driver}', 'CTPL Insurance',               true, true, 30,  7)
ON CONFLICT (code) DO NOTHING;
