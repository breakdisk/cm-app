-- Migration: 0018 — more than one lead on a home move
--
-- Model B (Joint Mission): a two-truck move can be taken by two single-truck
-- leads. The first to accept is the Mission Captain (surveys, runs the
-- customer side, holds the day's assignment); the Support Lead slot is then
-- offered for the second truck. An addendum that outgrows the trucks booked
-- adds an Emergency slot: another lead's truck, the same day.
--
-- So a move now has reservations by role — sole | captain | support |
-- emergency — each with the pay its lead accepted on the offer. One primary
-- (sole or captain) per move, and a lead holds at most one slot on a move.

ALTER TABLE dispatch.home_reservations
    ADD COLUMN IF NOT EXISTS id           UUID   NOT NULL DEFAULT gen_random_uuid(),
    ADD COLUMN IF NOT EXISTS role         TEXT   NOT NULL DEFAULT 'sole',
    ADD COLUMN IF NOT EXISTS payout_cents BIGINT;

ALTER TABLE dispatch.home_reservations DROP CONSTRAINT IF EXISTS home_reservations_pkey;
ALTER TABLE dispatch.home_reservations ADD PRIMARY KEY (id);

CREATE UNIQUE INDEX IF NOT EXISTS home_reservations_one_primary
    ON dispatch.home_reservations (shipment_id)
    WHERE role IN ('sole', 'captain') AND released_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS home_reservations_one_slot_per_lead
    ON dispatch.home_reservations (shipment_id, driver_id)
    WHERE released_at IS NULL;

-- Which slot an offer fills. An offer with no row here, on a home move, is
-- the primary one (offers made before this).
CREATE TABLE IF NOT EXISTS dispatch.home_offer_slots (
    offer_id    UUID        PRIMARY KEY,
    shipment_id UUID        NOT NULL,
    -- primary | support | emergency
    role        TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- A joint move's split of the lead's pay, priced by order-intake: the
-- captain's part and the support lead's.
ALTER TABLE dispatch.home_requirements
    ADD COLUMN IF NOT EXISTS captain_payout_cents BIGINT,
    ADD COLUMN IF NOT EXISTS support_payout_cents BIGINT;
