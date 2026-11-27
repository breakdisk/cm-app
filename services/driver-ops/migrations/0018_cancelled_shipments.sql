-- Migration: 0018 — shipments cancelled upstream
--
-- driver-ops never heard of a cancellation: a task for a cancelled shipment
-- stayed on the driver's list. The consumer now cancels it, and this row keeps
-- a task.assigned that arrives after the cancellation (another topic, another
-- consumer group) from creating one.

CREATE TABLE IF NOT EXISTS driver_ops.cancelled_shipments (
    shipment_id  UUID        PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    cancelled_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
