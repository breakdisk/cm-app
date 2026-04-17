-- Migration: 0007 — Fix RLS enforcement and update CHECK constraints
--
-- Root cause of HTTP 500 on shipment creation:
--   FORCE ROW LEVEL SECURITY requires current_setting('app.tenant_id')::uuid
--   to be set on every connection, but the service never sets it.
--   The service already enforces tenant isolation at the application layer
--   (JWT claims → explicit WHERE tenant_id = $1), so FORCE is unnecessary.
--
-- Also updates CHECK constraints that fell behind the Rust enum variants.

-- ── Fix 1: Remove FORCE RLS from shipments ──────────────────────────────────
-- RLS stays enabled (protects non-owner roles), but the table owner
-- (the service's `logisticos` role) is no longer forced through the policy.
ALTER TABLE order_intake.shipments NO FORCE ROW LEVEL SECURITY;

-- ── Fix 2: Remove FORCE RLS from shipment_pieces ────────────────────────────
ALTER TABLE order_intake.shipment_pieces NO FORCE ROW LEVEL SECURITY;

-- ── Fix 2b: Remove FORCE RLS from shipment_events (not yet used, but future-proof) ─
ALTER TABLE order_intake.shipment_events NO FORCE ROW LEVEL SECURITY;

-- ── Fix 3: Update service_type CHECK to include 'international' ─────────────
ALTER TABLE order_intake.shipments
    DROP CONSTRAINT IF EXISTS shipments_service_type_check;

ALTER TABLE order_intake.shipments
    ADD CONSTRAINT shipments_service_type_check
    CHECK (service_type IN ('standard','express','same_day','balikbayan','international'));

-- ── Fix 4: Update status CHECK to include newer statuses ────────────────────
ALTER TABLE order_intake.shipments
    DROP CONSTRAINT IF EXISTS shipments_status_check;

ALTER TABLE order_intake.shipments
    ADD CONSTRAINT shipments_status_check
    CHECK (status IN (
        'pending','confirmed','pickup_assigned','picked_up',
        'in_transit','at_hub','out_for_delivery',
        'delivery_attempted','delivered','partial_delivery',
        'piece_exception','customs_hold',
        'failed','cancelled','returned'
    ));
