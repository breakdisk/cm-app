-- Migration 0012: Add shipment_id to invoices for per-shipment billing clearance.
--
-- Per-shipment invoices (Track A Balikbayan pickup billing, COD receipts, etc.)
-- tag this column so the billing clearance gate at container departure can query
-- directly without a cross-service join.
--
-- Monthly batch invoices (ShipmentCharges) leave shipment_id NULL — they cover
-- many AWBs and cannot be keyed to a single shipment.
--
-- Nullable: the column is only populated for event-triggered per-shipment invoices.

ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS shipment_id UUID;

-- Partial index: fast lookup for clearance check queries.
-- Only indexes rows where shipment_id is set (i.e., per-shipment invoices).
CREATE INDEX IF NOT EXISTS invoices_shipment_id_idx
    ON payments.invoices (shipment_id)
    WHERE shipment_id IS NOT NULL;

COMMENT ON COLUMN payments.invoices.shipment_id IS
    'Populated for per-shipment invoices (Track A pickup, COD receipt). NULL for monthly batch invoices.';
