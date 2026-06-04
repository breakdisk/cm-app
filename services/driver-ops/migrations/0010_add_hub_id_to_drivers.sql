-- Add hub_id to drivers for hub scanner assignment.
-- No FK constraint — avoids cross-schema coupling with hub_ops.hubs.
ALTER TABLE driver_ops.drivers
    ADD COLUMN IF NOT EXISTS hub_id UUID NULL;
