-- Migration: 0017 — the lead's pay on a home move
--
-- A home move was offered with no payout shown ("the lead's pay is not
-- decided"). order-intake now prices it — base + m³ + road km, less the
-- platform's commission — and sends the net on shipment.created. It is shown
-- on the offer and carried on the task when the move is activated.

ALTER TABLE dispatch.home_requirements
    ADD COLUMN IF NOT EXISTS lead_payout_cents BIGINT;
