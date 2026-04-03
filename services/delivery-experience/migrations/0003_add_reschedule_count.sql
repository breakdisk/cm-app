-- Add reschedule_count to shipment_tracking to track customer-initiated reschedule requests.
ALTER TABLE tracking.shipment_tracking
    ADD COLUMN IF NOT EXISTS reschedule_count INTEGER NOT NULL DEFAULT 0;
