-- Driver task-card enrichment + driver performance counters.
--
-- tasks: merchant/sender display name, delivery category (icon + vehicle
-- matching), declared weight, and BOTH route ends so the app can render the
-- pickup→delivery route sketch on every card (each task row previously only
-- knew its own leg's address).
ALTER TABLE driver_ops.tasks
    ADD COLUMN IF NOT EXISTS merchant_name     TEXT   NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS delivery_category TEXT   NOT NULL DEFAULT 'parcel',
    ADD COLUMN IF NOT EXISTS weight_grams      BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS pickup_lat        DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS pickup_lng        DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS delivery_lat      DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS delivery_lng      DOUBLE PRECISION;

-- drivers: decline accountability + customer rating aggregate.
-- decline_count increments on every ASSIGNMENT_REJECTED event; at the ban
-- threshold (20) the consumer flips is_active=false (driver drops out of the
-- dispatch pool). rating_avg/rating_count are populated by the phase-2
-- feedback ingestion pipeline; surfaced in the driver app immediately.
ALTER TABLE driver_ops.drivers
    ADD COLUMN IF NOT EXISTS decline_count INT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS rating_avg    REAL,
    ADD COLUMN IF NOT EXISTS rating_count  INT  NOT NULL DEFAULT 0;
