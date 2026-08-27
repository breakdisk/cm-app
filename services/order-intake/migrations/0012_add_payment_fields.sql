-- Migration: 0012 — Order Intake: online-payment fields on shipments
--
-- `pending_dispatch_events` holds the AwbIssued/ShipmentCreated/ShipmentConfirmed
-- event payloads verbatim when payment_status = 'awaiting_payment', so the
-- payment-captured consumer can republish them unchanged rather than
-- reconstructing them from scratch (some of their fields, like sender_name,
-- aren't persisted anywhere else on this table).
--
-- No RLS: per migration 0011 (drop_decorative_rls), RLS was found decorative
-- on this table and is not re-added here.

ALTER TABLE order_intake.shipments
    ADD COLUMN payment_intent_id       UUID,
    ADD COLUMN payment_status          TEXT NOT NULL DEFAULT 'not_required'
                                        CHECK (payment_status IN (
                                            'not_required','awaiting_payment','paid','payment_failed'
                                        )),
    ADD COLUMN pending_dispatch_events JSONB,
    ADD COLUMN idempotency_key         TEXT;

-- Sweep target: shipments stuck awaiting payment past their TTL.
CREATE INDEX idx_shipments_awaiting_payment
    ON order_intake.shipments (payment_status, created_at)
    WHERE payment_status = 'awaiting_payment';

-- Idempotent re-submission of the same booking request must return the
-- existing shipment, scoped per tenant (two tenants could coincidentally
-- generate the same client-side UUID).
CREATE UNIQUE INDEX idx_shipments_idempotency
    ON order_intake.shipments (tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
