-- Migration: 0016 — shipments the customer (or the payment sweep) cancelled
--
-- Dispatch never heard of a cancellation before this. A shipment cancelled
-- while still queued stayed 'pending' — the sweep could hand it to a driver,
-- its open offer stayed grabbable, and a reserved whole-home move was still
-- activated for its lead twelve hours before a move that was not happening.
--
-- shipment.created and shipment.cancelled arrive on different topics through
-- different consumer groups, so a cancellation can be read first. This row is
-- the tombstone that keeps a late shipment.created from queueing it.

CREATE TABLE IF NOT EXISTS dispatch.shipment_cancellations (
    shipment_id  UUID        PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    cancelled_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
