-- Migration: 0015 — whole-home moves in dispatch
--
-- A home move is offered only to a lead onboarded for it who can meet it
-- (driver_ops.provider_profiles): the right coverage, enough trucks for a
-- multi-truck job, enough registered helpers, working that day and not
-- already full. The requirement comes on shipment.created.
--
-- Claiming a home move reserves the lead for the day; it does not assign
-- them. An assignment is active and one-per-driver, so a lead who took a move
-- three weeks out would otherwise be locked out of all work until then. The
-- reservation counts against their jobs a day, and becomes an assignment
-- twelve hours before the move.

CREATE TABLE IF NOT EXISTS dispatch.home_requirements (
    shipment_id   UUID        PRIMARY KEY,
    tenant_id     UUID        NOT NULL,
    trucks        INTEGER     NOT NULL,
    helpers       INTEGER     NOT NULL,
    crew_total    INTEGER     NOT NULL,
    large_estate  BOOLEAN     NOT NULL,
    international BOOLEAN     NOT NULL,
    move_at       TIMESTAMPTZ NOT NULL,
    -- The move's local date, as order-intake's calendar has it.
    move_date     DATE        NOT NULL,
    survey_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS dispatch.home_reservations (
    shipment_id  UUID        PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    driver_id    UUID        NOT NULL,
    move_date    DATE        NOT NULL,
    reserved_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    activated_at TIMESTAMPTZ,
    released_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_home_reservations_driver_day
    ON dispatch.home_reservations (driver_id, move_date) WHERE released_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_home_reservations_due
    ON dispatch.home_reservations (move_date) WHERE activated_at IS NULL AND released_at IS NULL;
