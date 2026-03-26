-- Migration: 0004 — Dev seed data with fixed UUIDs for reproducible testing.
-- All passwords: "LogisticOS1!" hashed with Argon2id.
-- DO NOT run in production — gated by environment check at migration time.

DO $$
BEGIN
  INSERT INTO identity.tenants (id, name, slug, subscription_tier, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Demo Logistics Co', 'demo', 'business', true
  ) ON CONFLICT DO NOTHING;

  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'admin@demo.com',
    '$argon2id$v=19$m=19456,t=2,p=1$sfbU/HB5EcNJtunKwP9QNQ$C51lutFAxk/U43haIN+U+FOCZrkIWGMkd48q/IVdVxY',
    'Admin', 'User',
    ARRAY['tenant_admin'], true, true
  ) ON CONFLICT DO NOTHING;

  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000004',
    '00000000-0000-0000-0000-000000000001',
    'driver@demo.com',
    '$argon2id$v=19$m=19456,t=2,p=1$sfbU/HB5EcNJtunKwP9QNQ$C51lutFAxk/U43haIN+U+FOCZrkIWGMkd48q/IVdVxY',
    'Ahmed', 'Al-Rashid',
    ARRAY['driver'], true, true
  ) ON CONFLICT DO NOTHING;

  INSERT INTO identity.users (id, tenant_id, email, password_hash, first_name, last_name, roles, email_verified, is_active)
  VALUES (
    '00000000-0000-0000-0000-000000000003',
    '00000000-0000-0000-0000-000000000001',
    'merchant@demo.com',
    '$argon2id$v=19$m=19456,t=2,p=1$sfbU/HB5EcNJtunKwP9QNQ$C51lutFAxk/U43haIN+U+FOCZrkIWGMkd48q/IVdVxY',
    'Sarah', 'Merchant',
    ARRAY['merchant'], true, true
  ) ON CONFLICT DO NOTHING;
END $$;
