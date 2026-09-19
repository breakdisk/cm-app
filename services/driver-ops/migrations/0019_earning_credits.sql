-- Migration: 0019 — pay a driver earns outside a delivered task
--
-- Earnings were delivered tasks' payouts, waiting pay and drop fees. Some
-- work is paid on its own:
--   survey_fee      — a home-move lead's survey, paid at sign-off whether or
--                     not the move goes ahead
--   support_share   — the support lead's part of a joint (two-truck) move
--   emergency_share — the lead who brought the extra truck an addendum needed
-- The kind is free text, validated by shape in the service: a new kind of
-- credited work is data, not a migration.
--
-- Every amount is net of the platform's commission; gross and commission are
-- kept beside it for the audit.

CREATE TABLE IF NOT EXISTS driver_ops.earning_credits (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    -- The driver's user id (drivers.id equals it since 0014).
    driver_id        UUID        NOT NULL,
    kind             TEXT        NOT NULL,
    -- The shipment the work was for.
    reference_id     UUID        NOT NULL,
    tracking_number  TEXT,
    gross_cents      BIGINT      NOT NULL,
    commission_cents BIGINT      NOT NULL,
    amount_cents     BIGINT      NOT NULL,
    credited_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Credited once: a retried call finds the row and adds nothing.
    CONSTRAINT earning_credits_once UNIQUE (driver_id, kind, reference_id),
    CONSTRAINT earning_credits_amounts CHECK (
        gross_cents >= 0 AND commission_cents >= 0 AND amount_cents = gross_cents - commission_cents
    )
);

CREATE INDEX IF NOT EXISTS earning_credits_by_driver_time
    ON driver_ops.earning_credits (driver_id, credited_at DESC);
