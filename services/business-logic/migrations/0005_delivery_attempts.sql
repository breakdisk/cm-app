-- Delivery-attempt ledger for the `DeliveryAttempted { attempts }` rule trigger.
--
-- No service publishes a running attempt count: driver-ops' `delivery.failed`
-- event (TaskFailed) carries no attempt number. business-logic therefore counts
-- failed delivery attempts per (tenant, shipment) itself.
--
-- Kafka delivery is at-least-once, so the primary key is the envelope event id:
-- a redelivered event hits ON CONFLICT DO NOTHING and is not counted twice.
-- Tenant is part of the key and of every lookup (ADR-0016: isolation is
-- application-layer).

CREATE TABLE IF NOT EXISTS business_logic.delivery_attempts (
    tenant_id   UUID        NOT NULL,
    event_id    UUID        NOT NULL,
    shipment_id UUID        NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_delivery_attempts_shipment
    ON business_logic.delivery_attempts (tenant_id, shipment_id);
