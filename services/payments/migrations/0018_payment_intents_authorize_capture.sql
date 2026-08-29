-- Migration: 0018 — Payments: authorize-then-capture, with void
--
-- Adds the two new terminal-ish statuses `PaymentIntent::authorize()` and
-- `PaymentIntent::void()` transition into/through. This is the foundation
-- for OmniDeliv's prepaid checkout: ring-fence funds when the order is
-- placed (Created/Pending -> Authorized, an NI `AUTH` order instead of
-- `SALE`), then either take the money once a courier accepts
-- (Authorized -> Captured, via `capture_authorized()`) or release the hold
-- if none does (Authorized -> Voided, via `void()`), so the customer is
-- never charged for an order nobody fulfilled.
--
-- Same pattern as migration 0016 (adding 'refunding'): Postgres has no
-- `ALTER ... ADD VALUE` for a plain CHECK constraint (only for native ENUM
-- types), so the existing constraint is dropped and re-added with the two
-- new values.

ALTER TABLE payments.payment_intents
    DROP CONSTRAINT IF EXISTS payment_intents_status_check;

ALTER TABLE payments.payment_intents
    ADD CONSTRAINT payment_intents_status_check
    CHECK (status IN (
        'created','pending','captured','failed','refunded','expired','refunding',
        'authorized','voided'
    ));
