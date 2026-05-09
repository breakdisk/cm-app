-- Track per-task delivery attempts separately from permanent failure.
-- attempt_count increments each time the driver marks "attempted but not
-- delivered" — customer gets a notification and the task re-queues for retry.
-- last_attempted_at records when the most recent non-completing attempt occurred.
ALTER TABLE driver_ops.tasks
    ADD COLUMN IF NOT EXISTS attempt_count     INT         NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_attempted_at TIMESTAMPTZ;
