-- Migration 0005: Add service_code and declared_value_cents to proofs_of_pickup.
--
-- service_code: billing routing classification carried from the shipment booking.
--   "balikbayan" → Track A driver ledger debit at pickup (payments consumer).
--   "standard"   → Track B weight-based surcharge at hub weigh-in.
--   Defaults to 'standard' for backward compat with rows created before this migration.
--
-- declared_value_cents: shipment declared value for Track A (Balikbayan) insurance /
--   base billing. NULL for standard parcel shipments.

ALTER TABLE pod.proofs_of_pickup
    ADD COLUMN IF NOT EXISTS service_code         TEXT         NOT NULL DEFAULT 'standard',
    ADD COLUMN IF NOT EXISTS declared_value_cents BIGINT;

COMMENT ON COLUMN pod.proofs_of_pickup.service_code IS
    'Billing classification: balikbayan|standard|express. Forwarded in PickupCaptured event.';
COMMENT ON COLUMN pod.proofs_of_pickup.declared_value_cents IS
    'Shipment declared value in cents for Track A (Balikbayan) billing. NULL for weight-based services.';
