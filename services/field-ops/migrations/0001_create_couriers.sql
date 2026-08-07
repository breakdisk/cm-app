-- Field-Ops platform tier. Owns the human operating in the field, shared by
-- every product that dispatches one. See ADR-0015.
CREATE SCHEMA IF NOT EXISTS field_ops;

-- TENANCY NOTE — read before adding an RLS policy here.
-- Other schemas in this repo run ENABLE ROW LEVEL SECURITY with a policy on
-- current_setting('app.tenant_id'). No service sets that variable, and services
-- connect as the schema owner, so PostgreSQL bypasses the policy entirely — it
-- neither filters nor fails. Where FORCE was added it broke reads and was
-- reverted (order-intake/0007, driver-ops/0005).
-- Isolation here is application-layer: every repository query filters on
-- tenant_id explicitly, enforced by a test. A decorative policy would imply a
-- database-level guarantee that does not exist, so this migration omits one.
-- Making RLS genuinely enforce is a platform-wide change and needs its own ADR.

CREATE TABLE IF NOT EXISTS field_ops.couriers (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    -- Links to identity.users. The courier is a platform-tier worker identity,
    -- distinct from the customer profile in the CDP.
    user_id        UUID        NOT NULL,
    first_name     TEXT        NOT NULL,
    last_name      TEXT        NOT NULL,
    phone          TEXT        NOT NULL,
    status         TEXT        NOT NULL DEFAULT 'offline'
                               CHECK (status IN ('offline','available','assigned','on_break')),
    vehicle_type   TEXT,
    zone           TEXT,
    -- CACHE ONLY. The authoritative position is the latest row in
    -- field_ops.courier_locations (migration 0003); these columns exist so a
    -- courier list renders without touching the time-series table. Never
    -- proximity-search on them — see the GiST index in 0003.
    last_lat       DOUBLE PRECISION,
    last_lng       DOUBLE PRECISION,
    last_seen_at   TIMESTAMPTZ,
    is_active      BOOLEAN     NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One courier record per user per tenant.
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_user
    ON field_ops.couriers (tenant_id, user_id);

-- Serves both "the tenant's active roster" and the status-narrowing half of a
-- supply lookup. One index, not two: an earlier draft added a second on
-- (tenant_id, status) WHERE status = 'available', which is a strict subset of
-- this one and buys nothing a planner cannot already get here — it would only
-- add write cost on a table that couriers update on every status change.
--
-- The geospatial half of a supply lookup runs against courier_latest_locations
-- (0003), which has the GiST index. There is deliberately NO btree on
-- (tenant_id, last_lat, last_lng): proximity search is ST_DWithin against a
-- geography, which a btree on two float columns cannot serve — it would sit
-- there looking useful while every search scanned.
CREATE INDEX IF NOT EXISTS idx_courier_tenant_status
    ON field_ops.couriers (tenant_id, status)
    WHERE is_active;
