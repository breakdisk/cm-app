-- Migration: 0006 — Update AWB service_char CHECK to use N instead of I
--
-- The letter I is excluded from the Luhn mod-34 charset (confusable with 1).
-- International service code changed from I to N (iNternational).

ALTER TABLE order_intake.awb_sequences
    DROP CONSTRAINT IF EXISTS awb_sequences_service_char_check;

ALTER TABLE order_intake.awb_sequences
    ADD CONSTRAINT awb_sequences_service_char_check
    CHECK (service_char IN ('S','E','D','B','N'));

-- Migrate any existing I rows to N
UPDATE order_intake.awb_sequences
    SET service_char = 'N'
    WHERE service_char = 'I';
