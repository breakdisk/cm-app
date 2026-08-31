-- Partial capture — ADR-0017's acceptance barrier.
--
-- A foodcourt order authorizes the whole basket, then some stalls accept and
-- some refuse. Capturing the full authorized amount would take money for food
-- nobody is making; capturing nothing would refuse the whole table because one
-- stall was closed. The barrier captures the accepted subtotal.
--
-- `amount_cents` stays what was AUTHORIZED and is never rewritten — `refund()`
-- and every reconciliation read it, and an authorized amount that shifted after
-- the fact would make a hold impossible to match against its own capture.
-- What was actually taken is a separate fact.
--
-- NULL for every intent captured before this existed, and for any intent not
-- captured at all. NULL on a `captured` intent therefore means "captured in
-- full, before partial capture existed" — read it as `amount_cents`.

ALTER TABLE payments.payment_intents
    ADD COLUMN IF NOT EXISTS captured_amount_cents BIGINT;

-- Cannot capture more than was ring-fenced, and a zero capture is a void
-- wearing a capture's name — the caller must say which it meant.
ALTER TABLE payments.payment_intents
    DROP CONSTRAINT IF EXISTS payment_intent_capture_within_authorization;

ALTER TABLE payments.payment_intents
    ADD CONSTRAINT payment_intent_capture_within_authorization
    CHECK (
        captured_amount_cents IS NULL
        OR (captured_amount_cents > 0 AND captured_amount_cents <= amount_cents)
    );
