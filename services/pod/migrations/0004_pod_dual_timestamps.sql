-- Add POP & telemetry fields to the existing pod.proofs table.
-- Both columns are nullable / default-false so the migration is backward-compatible
-- with all existing POD rows (pre-migration rows read as false / NULL).
--
-- out_of_bounds_handover: soft audit flag set when driver was > 50m from delivery address.
-- device_timestamp:       hardware clock timestamp from driver device at capture time.

ALTER TABLE pod.proofs
    ADD COLUMN IF NOT EXISTS out_of_bounds_handover BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS device_timestamp       TIMESTAMPTZ;
