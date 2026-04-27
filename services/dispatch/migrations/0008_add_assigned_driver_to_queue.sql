-- Migration 0008: record which driver was assigned to each dispatch_queue row.
-- NULL while pending; populated by mark_dispatched when the shipment is assigned.
ALTER TABLE dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS assigned_driver_id UUID;
