-- Partial refunds: the penalty policy's retention.
--
-- A cancellation inside the late window keeps a fee and refunds the rest. The
-- refund obligation used to be a timestamp only (`refund_requested_at`, 0016),
-- and `sweep_pending_refunds` retried it by refunding the whole capture. A
-- partial refund whose gateway call failed would have been retried as a full
-- one, handing the retention back.
--
-- `refund_requested_cents` is what is owed, recorded with the obligation. NULL
-- means the whole capture, which is every obligation recorded before this
-- existed. `refunded_cents` is what actually came back. NULL on a `refunded`
-- intent means refunded in full, before partial refunds existed.

ALTER TABLE payments.payment_intents
    ADD COLUMN IF NOT EXISTS refund_requested_cents BIGINT,
    ADD COLUMN IF NOT EXISTS refunded_cents         BIGINT;

-- A refund of zero is not a refund: a fully retained cancellation records no
-- obligation at all. Neither figure may exceed what was authorized.
ALTER TABLE payments.payment_intents
    DROP CONSTRAINT IF EXISTS payment_intent_refund_within_authorization;

ALTER TABLE payments.payment_intents
    ADD CONSTRAINT payment_intent_refund_within_authorization
    CHECK (
        (refund_requested_cents IS NULL
            OR (refund_requested_cents > 0 AND refund_requested_cents <= amount_cents))
        AND
        (refunded_cents IS NULL
            OR (refunded_cents > 0 AND refunded_cents <= amount_cents))
    );
