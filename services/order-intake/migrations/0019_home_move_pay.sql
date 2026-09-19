-- Migration: 0019 — what the lead is paid for a home move
--
-- Priced at booking: base + packed m³ + road km (or the move's fare when no
-- pay rates are set), never more than the fare, less the platform's
-- commission (20% by default). The net is what the lead is shown on the offer
-- and paid on delivery; gross and commission are kept for the audit. A
-- move booked before this has 0 here and is offered with no pay shown.

ALTER TABLE order_intake.home_moves
    ADD COLUMN IF NOT EXISTS lead_gross_cents      BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS lead_commission_cents BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS lead_payout_cents     BIGINT NOT NULL DEFAULT 0;
