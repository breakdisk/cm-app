-- Feedback is now written only for a real shipment, with tenant, shipment and
-- driver read from its tracking record (the public endpoint used to accept any
-- tracking number and insert rows with a NULL tenant).
--
-- Existing rows are left alone: older rows have no tenant/shipment/driver and
-- there is nothing in this table to backfill them from.

ALTER TABLE delivery_experience.feedback
    ADD COLUMN IF NOT EXISTS shipment_id UUID,
    ADD COLUMN IF NOT EXISTS driver_id   UUID;

-- One feedback per shipment. Partial: legacy rows have NULL shipment_id.
CREATE UNIQUE INDEX IF NOT EXISTS feedback_one_per_shipment
    ON delivery_experience.feedback (shipment_id)
    WHERE shipment_id IS NOT NULL;

-- Per-driver rating rollups.
CREATE INDEX IF NOT EXISTS feedback_driver_idx
    ON delivery_experience.feedback (driver_id)
    WHERE driver_id IS NOT NULL;

-- NOT VALID: enforced for every new/updated row; existing NULL-tenant rows are
-- not scanned and do not fail the migration. Run VALIDATE CONSTRAINT after any
-- legacy cleanup.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'feedback_tenant_required'
    ) THEN
        ALTER TABLE delivery_experience.feedback
            ADD CONSTRAINT feedback_tenant_required
            CHECK (tenant_id IS NOT NULL) NOT VALID;
    END IF;
END $$;
