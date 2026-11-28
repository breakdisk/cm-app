-- Capture/void idempotency and crash safety.
--
-- `capturing` / `voiding` are claim states: authorized -> capturing|voiding is
-- an atomic conditional UPDATE taken BEFORE the gateway call, so a capture and a
-- void (or two captures) can never both reach the gateway. A claim whose holder
-- died is reclaimable once `updated_at` is older than the lease (see
-- PgPaymentIntentRepository). `gateway_capture_ref` lets a retried capture
-- return the original capture reference without a second gateway call.

ALTER TABLE payments.payment_intents
    DROP CONSTRAINT IF EXISTS payment_intents_status_check;

ALTER TABLE payments.payment_intents
    ADD CONSTRAINT payment_intents_status_check
    CHECK (status IN (
        'created','pending','captured','failed','refunded','expired','refunding',
        'authorized','voided','capturing','voiding'
    ));

ALTER TABLE payments.payment_intents
    ADD COLUMN IF NOT EXISTS gateway_capture_ref TEXT,
    -- Ambiguous gateway error (timeout/5xx/transport) recorded while a
    -- capturing/voiding claim is deliberately KEPT; for ops triage.
    ADD COLUMN IF NOT EXISTS last_gateway_error   TEXT;
