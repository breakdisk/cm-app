-- Migration: 0016 — a booked whole-home move
--
-- The shipment row carries what every job carries (route, status, payment,
-- the booked move time in scheduled_pickup_at). This is what only a home move
-- has: the property, the inventory as priced, the plan, and the survey.
-- Written once at booking from the signed quote; the survey corrects the
-- inventory in a later step, never by rewriting this row in place.

CREATE TABLE IF NOT EXISTS order_intake.home_moves (
    shipment_id      UUID        PRIMARY KEY,
    tenant_id        UUID        NOT NULL,
    account_id       UUID        NOT NULL,
    property         JSONB       NOT NULL,
    items            JSONB       NOT NULL,
    plan             TEXT        NOT NULL,
    distance_centikm BIGINT      NOT NULL,
    survey_required  BOOLEAN     NOT NULL,
    survey_at        TIMESTAMPTZ,
    move_at          TIMESTAMPTZ NOT NULL,
    total_cents      BIGINT      NOT NULL,
    currency         TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (survey_required = (survey_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_home_moves_account ON order_intake.home_moves (tenant_id, account_id, move_at DESC);
