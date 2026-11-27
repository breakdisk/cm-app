-- Who booked each shipment, so a customer reads only their own tracking.
--
-- `GET /v1/tracking/:shipment_id` and `GET /v1/tracking` checked the tenant
-- only: any customer holding shipments:read could read another customer's
-- record, driver phone included, or list the whole tenant's.
--
-- Stamped from shipment.created's merchant_id (the booking user). Rows
-- projected before this have no owner and stay NULL: there is no source to
-- backfill from inside this service, and owner-scoped readers are refused
-- them. Operators are unaffected, and the public tracking-number page still
-- serves everyone.

ALTER TABLE tracking.shipment_tracking
    ADD COLUMN IF NOT EXISTS owner_id UUID;

-- A customer's own list, newest first.
CREATE INDEX IF NOT EXISTS tracking_by_tenant_owner_created
    ON tracking.shipment_tracking (tenant_id, owner_id, created_at DESC)
    WHERE owner_id IS NOT NULL;
