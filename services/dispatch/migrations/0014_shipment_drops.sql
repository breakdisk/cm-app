-- A driver who dropped a shipment is not offered it again.
--
-- driver-ops publishes `driver.job.dropped`; dispatch cancels that driver's
-- assignment and puts the shipment back in the queue. Without this table the
-- 90-second sweep would hand it straight back to the same driver — they are
-- free again, and usually the nearest.
--
-- It is also what makes the requeue replay-safe: a shipment is requeued only
-- when its drop row is new, so a redelivered event cannot pull a shipment back
-- from the driver it has been re-dispatched to since.
CREATE TABLE IF NOT EXISTS dispatch.shipment_drops (
    shipment_id UUID        NOT NULL,
    driver_id   UUID        NOT NULL,
    route_id    UUID        NOT NULL,
    tenant_id   UUID        NOT NULL,
    dropped_at  TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (shipment_id, driver_id, route_id)
);
