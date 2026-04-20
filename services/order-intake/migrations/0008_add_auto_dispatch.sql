-- Add auto_dispatch flag — independent of booked_by_customer.
--
-- booked_by_customer keeps its billing semantics (customer pre-authorised card → issue
-- PaymentReceipt at POD). auto_dispatch is a separate signal that tells the dispatch
-- shipment_consumer to call quick_dispatch immediately, without waiting for a human.
--
-- Defaults:
--   customer role  → true  (customer app self-booking)
--   merchant role  → true  (agentic-first: auto-assign on intake, admin can override)
--   admin role     → false (manual dispatch console flow)

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS auto_dispatch BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN order_intake.shipments.auto_dispatch
    IS 'When true, shipment_consumer in dispatch service auto-assigns the best available driver on creation.';
