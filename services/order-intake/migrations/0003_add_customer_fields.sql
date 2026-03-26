-- Add customer contact fields to shipments for dispatch denormalization
ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS customer_name  TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS customer_phone TEXT NOT NULL DEFAULT '';

-- Remove the defaults after backfill (new inserts will always provide them)
ALTER TABLE order_intake.shipments
    ALTER COLUMN customer_name  DROP DEFAULT,
    ALTER COLUMN customer_phone DROP DEFAULT;
