-- A stored delivery deadline and the moment it was found missed.
-- Additive and nullable: no backfill, so shipments that predate this have no
-- deadline and can never breach (no alert storm for historical open shipments).
ALTER TABLE order_intake.shipments ADD COLUMN IF NOT EXISTS sla_due_at      TIMESTAMPTZ;
ALTER TABLE order_intake.shipments ADD COLUMN IF NOT EXISTS sla_breached_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_shipments_sla_open
    ON order_intake.shipments (sla_due_at)
    WHERE sla_breached_at IS NULL AND sla_due_at IS NOT NULL;
