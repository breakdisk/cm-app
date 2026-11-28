-- Dedupe ledger for customer ratings folded into driver_ops.drivers
-- (rating_avg / rating_count, migration 0011).
--
-- `logisticos.delivery.feedback.submitted` is delivered at-least-once. The
-- consumer inserts here and bumps the driver's aggregate in ONE transaction, so
-- a redelivery hits the primary key and changes nothing. Keyed by shipment (one
-- feedback per shipment is enforced upstream in delivery-experience), so even a
-- re-published event with a fresh event id cannot count twice.
CREATE TABLE IF NOT EXISTS driver_ops.driver_rating_events (
    tenant_id   UUID        NOT NULL,
    shipment_id UUID        NOT NULL,
    event_id    UUID        NOT NULL,
    driver_id   UUID        NOT NULL,
    rating      SMALLINT    NOT NULL CHECK (rating BETWEEN 1 AND 5),
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, shipment_id)
);

CREATE INDEX IF NOT EXISTS driver_rating_events_driver_idx
    ON driver_ops.driver_rating_events (tenant_id, driver_id);
