-- Driver task-card enrichment: merchant/sender display name, delivery category
-- (icon + vehicle matching), and declared weight. All additive with defaults so
-- existing rows and old-event upserts keep working.
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS merchant_name     TEXT   NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS delivery_category TEXT   NOT NULL DEFAULT 'parcel',
    ADD COLUMN IF NOT EXISTS weight_grams      BIGINT NOT NULL DEFAULT 0;
