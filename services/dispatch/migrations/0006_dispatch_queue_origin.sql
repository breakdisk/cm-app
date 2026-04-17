-- Migration: 0006 — Add structured origin (sender) address to dispatch_queue.
-- Required so dispatch can emit a pickup TaskAssigned event with a real origin
-- address (not just the destination). Customer-app bookings need a pickup task
-- before the delivery task, and the origin info must flow:
--   ShipmentCreated.origin_* → dispatch_queue.origin_* → TaskAssigned (pickup leg)

ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_address_line1 TEXT             NOT NULL DEFAULT '';
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_city          TEXT             NOT NULL DEFAULT '';
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_province      TEXT             NOT NULL DEFAULT '';
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_postal_code   TEXT             NOT NULL DEFAULT '';
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_lat           DOUBLE PRECISION;
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS origin_lng           DOUBLE PRECISION;
