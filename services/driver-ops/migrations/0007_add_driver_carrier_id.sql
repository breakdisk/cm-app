-- Add carrier_id to drivers — the ADR-0013 partner-carrier link.
--
-- NULL = tenant-own driver (not part of an external carrier).
-- Non-NULL = driver belongs to a third-party carrier; used for
-- per-carrier manifest aggregation in the partner portal.
--
-- Intentionally NOT a FK to carrier.carriers — that's in a different
-- service schema, and a cross-service FK couples the two. Referential
-- integrity is enforced by the carrier service when drivers are
-- assigned; driver-ops just stores the UUID.

ALTER TABLE driver_ops.drivers
    ADD COLUMN IF NOT EXISTS carrier_id UUID NULL;

-- Partial index so carrier-scoped manifest queries stay cheap as the
-- table grows. Omits the common NULL case.
CREATE INDEX IF NOT EXISTS idx_drivers_carrier_id
    ON driver_ops.drivers(carrier_id)
    WHERE carrier_id IS NOT NULL;
