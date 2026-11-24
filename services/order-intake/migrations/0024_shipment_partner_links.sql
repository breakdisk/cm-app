-- Migration: 0024 — which partner handles each shipment
--
-- order-intake has no carrier column, so a partner token had nothing to be
-- scoped on and read every shipment in the tenant. This table is the missing
-- link, projected from events: a carrier allocation (carrier.allocated) or a
-- task given to one of a carrier's drivers (task.assigned with a
-- driver_carrier_id) links the shipment to that carrier.
--
-- Many rows per shipment is deliberate: a shipment handed from one carrier to
-- another keeps both links, so neither loses sight of work it did. A partner
-- reads a shipment if ANY link names it.
--
-- `source` is a free label for diagnosis (carrier_allocated, task_assigned,
-- backfill), not a CHECK: a new way of linking is data, not DDL.

CREATE TABLE IF NOT EXISTS order_intake.shipment_partner_links (
    tenant_id    UUID        NOT NULL,
    shipment_id  UUID        NOT NULL,
    carrier_id   UUID        NOT NULL,
    source       TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (shipment_id, carrier_id)
);

-- A partner's list is "my shipments, newest first".
CREATE INDEX IF NOT EXISTS idx_shipment_partner_links_carrier
    ON order_intake.shipment_partner_links (tenant_id, carrier_id, created_at DESC);
