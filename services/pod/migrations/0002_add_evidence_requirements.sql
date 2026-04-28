-- Add task-level evidence requirement flags to the pods table.
-- DEFAULT TRUE preserves the existing behaviour for all rows created before this migration:
-- they still require photo/signature as before. New rows created by dispatch tasks that
-- don't mandate evidence (e.g. OTP-only or low-risk deliveries) will be set to FALSE,
-- allowing geofence alone to satisfy completeness.

ALTER TABLE pods
    ADD COLUMN IF NOT EXISTS requires_photo     BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS requires_signature BOOLEAN NOT NULL DEFAULT TRUE;
