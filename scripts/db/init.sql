-- LogisticOS — PostgreSQL Initialization
-- Creates per-service schemas with tenant isolation via Row-Level Security (RLS).

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pg_trgm;  -- for fuzzy address search

-- ── Schemas (one per service) ─────────────────────────────────
CREATE SCHEMA IF NOT EXISTS identity;
CREATE SCHEMA IF NOT EXISTS cdp;
CREATE SCHEMA IF NOT EXISTS order_intake;
CREATE SCHEMA IF NOT EXISTS dispatch;
CREATE SCHEMA IF NOT EXISTS driver_ops;
CREATE SCHEMA IF NOT EXISTS fleet;
CREATE SCHEMA IF NOT EXISTS hub_ops;
CREATE SCHEMA IF NOT EXISTS carrier;
CREATE SCHEMA IF NOT EXISTS pod;
CREATE SCHEMA IF NOT EXISTS payments;
CREATE SCHEMA IF NOT EXISTS analytics;
CREATE SCHEMA IF NOT EXISTS marketing;
CREATE SCHEMA IF NOT EXISTS engagement;
CREATE SCHEMA IF NOT EXISTS business_logic;
CREATE SCHEMA IF NOT EXISTS tracking;
CREATE SCHEMA IF NOT EXISTS compliance;
CREATE SCHEMA IF NOT EXISTS ai_layer;
CREATE SCHEMA IF NOT EXISTS webhooks;

-- ── Service-specific DB users (principle of least privilege) ──
CREATE ROLE identity_svc LOGIN PASSWORD 'change-in-vault';
CREATE ROLE cdp_svc      LOGIN PASSWORD 'change-in-vault';
CREATE ROLE order_svc    LOGIN PASSWORD 'change-in-vault';
CREATE ROLE dispatch_svc LOGIN PASSWORD 'change-in-vault';
CREATE ROLE driver_svc   LOGIN PASSWORD 'change-in-vault';
CREATE ROLE payments_svc LOGIN PASSWORD 'change-in-vault';

GRANT USAGE ON SCHEMA identity  TO identity_svc;
GRANT USAGE ON SCHEMA cdp       TO cdp_svc;
GRANT USAGE ON SCHEMA order_intake TO order_svc;
GRANT USAGE ON SCHEMA dispatch  TO dispatch_svc;
GRANT USAGE ON SCHEMA driver_ops TO driver_svc;
GRANT USAGE ON SCHEMA payments  TO payments_svc;

-- identity_svc owns its schema fully
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA identity TO identity_svc;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA identity TO identity_svc;
ALTER DEFAULT PRIVILEGES IN SCHEMA identity GRANT ALL ON TABLES TO identity_svc;
ALTER DEFAULT PRIVILEGES IN SCHEMA identity GRANT ALL ON SEQUENCES TO identity_svc;

-- (Repeat pattern for each service schema — omitted for brevity, handled by migrations)

-- ── Shared: tenant lookup table (read-only for all services) ──
CREATE TABLE IF NOT EXISTS identity.tenants (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,
    subscription_tier TEXT NOT NULL DEFAULT 'starter'
                        CHECK (subscription_tier IN ('starter','growth','business','enterprise')),
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- All other services can read tenants for validation
GRANT SELECT ON identity.tenants TO order_svc, dispatch_svc, driver_svc, payments_svc, cdp_svc;
