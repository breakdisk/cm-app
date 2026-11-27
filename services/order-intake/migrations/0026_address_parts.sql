-- Migration: 0026 — building, unit and area of the destination
--
-- UAE addresses are described by building, unit and area as much as by street.
-- Kept as the customer gave them, with the destination; never geocoded.
-- Existing shipments read NULL.

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS dest_building TEXT,
    ADD COLUMN IF NOT EXISTS dest_unit     TEXT,
    ADD COLUMN IF NOT EXISTS dest_area     TEXT;
