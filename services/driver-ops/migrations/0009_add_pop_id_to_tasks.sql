-- Proof of Pickup reference on tasks.
-- Pickup tasks store the pop_id once the driver submits their POP,
-- mirroring how delivery tasks store pod_id after POD submission.
ALTER TABLE driver_ops.tasks
    ADD COLUMN IF NOT EXISTS pop_id UUID;
