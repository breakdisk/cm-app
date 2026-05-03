-- Add customer_id to dispatch_queue for engagement service notifications
-- The engagement service needs customer_id to route driver.assigned notifications to the correct recipient
--
-- IF NOT EXISTS is required: the schema-isolated migrator (libs/common/src/migrations.rs)
-- pre-seeds an empty <schema>._sqlx_migrations table on first roll-out of ADR-0012, so
-- this migration can re-run against a database where the column was already added by a
-- prior run that tracked state in public._sqlx_migrations.
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS customer_id UUID NOT NULL DEFAULT gen_random_uuid();

-- Add index for customer_id lookups during driver assignment
CREATE INDEX IF NOT EXISTS idx_dispatch_queue_customer
    ON dispatch.dispatch_queue (customer_id);

-- Add customer_email and customer_phone columns if they don't exist (for completeness)
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS customer_email TEXT,
    ADD COLUMN IF NOT EXISTS customer_phone TEXT NOT NULL DEFAULT '';

-- Add tracking_number if missing
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS tracking_number TEXT;

-- Add origin address fields if missing
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS origin_address_line1 TEXT DEFAULT '' NOT NULL,
    ADD COLUMN IF NOT EXISTS origin_city TEXT DEFAULT '' NOT NULL,
    ADD COLUMN IF NOT EXISTS origin_province TEXT DEFAULT '' NOT NULL,
    ADD COLUMN IF NOT EXISTS origin_postal_code TEXT DEFAULT '' NOT NULL,
    ADD COLUMN IF NOT EXISTS origin_lat DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS origin_lng DOUBLE PRECISION;
