-- Migration: 0016 — Payments: durable refund tracking + atomic refund claim
--
-- Closes two money-safety gaps in the Network International refund path:
--
-- Gap 1 (a failed refund is lost): `refund_requested_at` durably records the
-- instant a refund became owed (a captured shipping_fee intent on a
-- cancelled shipment). `ShipmentCancelledConsumer` writes this BEFORE
-- attempting the gateway call, so the obligation survives a crash mid-call —
-- a new periodic sweep (`PaymentIntentService::sweep_pending_refunds`,
-- symmetric to the existing `sweep_expired`) retries every `captured` intent
-- with this column set. The partial index below is exactly that sweep's
-- query shape.
--
-- Gap 2 (concurrent refunds can double-call the gateway): `save()` was a
-- blind full-row upsert with no concurrency guard, so two racing callers
-- (the cancellation consumer and the new retry sweep can genuinely overlap)
-- could both pass the in-memory `status == Captured` check and both hit the
-- live gateway. `refunding` is a new intermediate status atomically claimed
-- via `UPDATE ... SET status = 'refunding' WHERE status = 'captured'` —
-- only the caller whose UPDATE actually affects a row may call the gateway.
-- Postgres has no `ALTER ... ADD VALUE` for a plain CHECK constraint (that's
-- only for native ENUM types), so the existing constraint is dropped and
-- re-added with the new value — matches the pattern already used in
-- services/hub-ops/migrations/0006_extend_container_status.sql.

ALTER TABLE payments.payment_intents
    ADD COLUMN refund_requested_at TIMESTAMPTZ;

-- The pending-refund sweep's exact query: captured intents with a recorded,
-- unfulfilled refund obligation. Once a row transitions to 'refunding' or
-- 'refunded' it naturally drops out (status no longer 'captured'), so this
-- index never needs to know about those statuses.
CREATE INDEX idx_payment_intents_pending_refunds
    ON payments.payment_intents (refund_requested_at)
    WHERE status = 'captured' AND refund_requested_at IS NOT NULL;

ALTER TABLE payments.payment_intents
    DROP CONSTRAINT IF EXISTS payment_intents_status_check;

ALTER TABLE payments.payment_intents
    ADD CONSTRAINT payment_intents_status_check
    CHECK (status IN (
        'created','pending','captured','failed','refunded','expired','refunding'
    ));
