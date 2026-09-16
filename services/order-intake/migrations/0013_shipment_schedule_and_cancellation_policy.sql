-- Cancellation policy inputs (mobile design part 2, penalty policy).
--
-- scheduled_pickup_at: when the job is booked to start. The cancellation tier is
-- measured against it. NULL for every shipment booked without a slot, which the
-- policy deliberately does not price (decided 2026-09-14): those keep the old
-- rule, free and only before pickup assignment.
--
-- cancellation_policy_version: the version in force when the customer booked.
--
-- booking_amount_cents / booking_currency: the verified quote total. Display only,
-- for the cancellation preview. Payments computes the retention on what it
-- actually captured, never on this.

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS scheduled_pickup_at         TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancellation_policy_version TEXT,
    ADD COLUMN IF NOT EXISTS booking_amount_cents        BIGINT,
    ADD COLUMN IF NOT EXISTS booking_currency            TEXT;
