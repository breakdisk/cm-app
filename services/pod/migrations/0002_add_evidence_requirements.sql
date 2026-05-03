-- Add task-level evidence requirement flags to the proofs table.
-- DEFAULT TRUE preserves the existing behaviour for all rows created before this migration:
-- they still require photo/signature as before. New rows created by dispatch tasks that
-- don't mandate evidence (e.g. OTP-only or low-risk deliveries) will be set to FALSE,
-- allowing geofence alone to satisfy completeness.
--
-- Table is pod.proofs (created in 0001_create_pod_tables.sql). Using the schema-qualified
-- name explicitly so this migration doesn't depend on `search_path` resolution — the
-- migration runner connects via a pool whose `after_connect` may or may not have set
-- search_path yet, depending on initialisation order.

ALTER TABLE pod.proofs
    ADD COLUMN IF NOT EXISTS requires_photo     BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS requires_signature BOOLEAN NOT NULL DEFAULT TRUE;
