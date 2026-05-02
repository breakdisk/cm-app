-- Add shipment_id to driver_assignments so that reject_assignment can
-- re-queue the correct shipment without a cross-table join.
-- Nullable: existing rows and multi-stop auto-assign assignments have no
-- single shipment_id (the route owns multiple shipments via stops).
-- Populated only for quick_dispatch (single-shipment) assignments.

ALTER TABLE dispatch.driver_assignments
    ADD COLUMN IF NOT EXISTS shipment_id UUID;

CREATE INDEX IF NOT EXISTS idx_assignments_shipment_id
    ON dispatch.driver_assignments (shipment_id)
    WHERE shipment_id IS NOT NULL;
