-- Migration: add customer_confirmed_at to tracking.shipment_tracking
-- Allows customers to self-confirm receipt of their package via the app.
-- Supplements driver POD capture; used for NPS triggers, dispute resolution,
-- and engagement automation (feedback request after confirmation).

ALTER TABLE tracking.shipment_tracking
    ADD COLUMN IF NOT EXISTS customer_confirmed_at TIMESTAMPTZ;

COMMENT ON COLUMN tracking.shipment_tracking.customer_confirmed_at
    IS 'Timestamp at which the customer confirmed receipt via the LogisticOS app. NULL = not yet self-confirmed.';
