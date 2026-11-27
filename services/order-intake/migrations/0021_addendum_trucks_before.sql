-- Migration: 0021 — how many trucks a move had before its addendum was paid
--
-- A paid addendum that needs more trucks than were booked (a major overflow)
-- brings an extra truck from another lead the same day, who shares the
-- addendum's pay. Recording the trucks before settlement is what tells, at
-- delivery, how many extra trucks the addendum needed and whose they were.

ALTER TABLE order_intake.home_addenda
    ADD COLUMN IF NOT EXISTS trucks_before INTEGER;
